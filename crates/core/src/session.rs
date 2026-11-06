//! Handles one phone connection, from the first handshake to receiving its video.

use crate::engine::{Inner, Phase, SessionHandle, UserError};
use crate::pipeline::{self, is_keyframe};
use crate::protocol::*;
use axum::extract::ws::{Message, WebSocket};
use futures_util::stream::SplitStream;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use rtc::media::io::sample_builder::SampleBuilder;
use rtc::peer_connection::configuration::media_engine::MIME_TYPE_H264;
use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use rtc::rtp::codec::h264::H264Packet;
use rtc::rtp_transceiver::rtp_sender::{RTCPFeedback, RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
use tokio::sync::{mpsc, Notify};
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    register_default_interceptors, MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfiguration, RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceConnectionState, RTCIceGatheringState,
    RTCIceServer, RTCPeerConnectionIceEvent, RTCPeerConnectionState, RTCSessionDescription, Registry,
};

const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// Most frames allowed to wait for decoding before new ones are skipped.
const MAX_PENDING_FRAMES: usize = 8;
const KEYFRAME_REQUEST_GAP: Duration = Duration::from_millis(300);

/// Sets up calls to accept H.264 Baseline, Main and High, the formats phones encode in hardware and we can decode.
fn media_engine() -> crate::Result<(MediaEngine, Registry)> {
    let mut media = MediaEngine::default();
    // Only offers full-picture refresh requests, so the phone works out its own sending speed.
    let feedback = vec![RTCPFeedback { typ: "ccm".into(), parameter: "fir".into() }];
    for (pt, fmtp) in [
        (102, "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"),
        (108, "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"),
        (110, "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=4d001f"),
        (112, "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=64001f"),
        (114, "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640c1f"),
    ] {
        media.register_codec(
            RTCRtpCodecParameters {
                rtp_codec: RTCRtpCodec {
                    mime_type: MIME_TYPE_H264.into(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: fmtp.into(),
                    rtcp_feedback: feedback.clone(),
                },
                payload_type: pt,
                ..Default::default()
            },
            RtpCodecKind::Video,
        )?;
    }
    // Adds standard network feedback so the phone can adjust its video quality.
    let registry = register_default_interceptors(Registry::new(), &mut media)?;
    Ok((media, registry))
}

#[derive(Debug)]
enum End {
    /// The connection was ended on purpose, so no reconnect is expected.
    Bye,
    /// A newer connection from the same phone took over.
    Replaced,
    /// The connection dropped and the phone will try again.
    Lost,
    Rejected,
}

pub async fn run(inner: Arc<Inner>, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<DesktopMessage>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let Ok(text) = serde_json::to_string(&msg) else { continue };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });
    serve(&inner, &tx, &mut stream).await;
    drop(tx);
    let _ = writer.await;
}

async fn serve(inner: &Arc<Inner>, tx: &mpsc::UnboundedSender<DesktopMessage>, stream: &mut SplitStream<WebSocket>) {
    let Ok(Some(PhoneMessage::Hello { protocol_version, token, device_id, device_secret, device_name, user_agent })) =
        tokio::time::timeout(HELLO_TIMEOUT, next_message(inner, stream)).await
    else {
        inner.debug(|| format!("no hello within {}s, closing socket", HELLO_TIMEOUT.as_secs()));
        return;
    };
    inner.debug(|| {
        let kind = if token.is_some() { "pairing" } else if device_id.is_some() { "reconnect" } else { "no credentials" };
        format!("hello: {kind}, protocol {protocol_version}, device \"{device_name}\", {user_agent}")
    });

    let auth = if protocol_version != PROTOCOL_VERSION {
        Err(RejectReason::VersionMismatch)
    } else if let Some(token) = token {
        let consumed = inner.pairing.lock().unwrap().consume(&token);
        consumed.map(|_| {
            let (device, secret) = inner.devices.lock().unwrap().register(&device_name, &user_agent);
            (device, Some(secret))
        })
    } else if let (Some(id), Some(secret)) = (device_id, device_secret) {
        inner.devices.lock().unwrap().verify(&id, &secret).map(|d| (d, None))
    } else {
        Err(RejectReason::InvalidToken)
    };

    let (device, new_secret) = match auth {
        Ok(v) => v,
        Err(reason) => {
            inner.log(format!("rejected connection: {reason:?}"));
            if matches!(reason, RejectReason::ExpiredToken) {
                inner.update(|s| s.error = Some(UserError { code: "expired_token", detail: String::new() }));
            }
            let _ = tx.send(DesktopMessage::Reject { reason });
            return;
        }
    };

    let id = inner.session_ids.fetch_add(1, Ordering::SeqCst) + 1;
    let close = Arc::new(Notify::new());
    let busy = {
        let mut current = inner.session.lock().unwrap();
        let active = matches!(inner.phase(), Phase::Connected | Phase::Connecting);
        let busy = current.as_ref().is_some_and(|old| old.device_id != device.id && active);
        if !busy {
            if let Some(old) = current.take() {
                old.end("replaced");
            }
            *current = Some(SessionHandle { id, device_id: device.id.clone(), tx: tx.clone(), close: close.clone() });
        }
        busy
    };
    if busy {
        inner.log("rejected second phone: busy");
        let _ = tx.send(DesktopMessage::Reject { reason: RejectReason::Busy });
        return;
    }
    if new_secret.is_some() {
        inner.emit_devices();
    }
    inner.log(format!("phone connected: {}", device.name));
    inner.debug(|| format!("session {id} started, {} ICE server(s)", inner.network.lock().unwrap().ice_servers.len()));
    let ice_servers = inner.network.lock().unwrap().ice_servers.clone();
    let _ = tx.send(DesktopMessage::Welcome {
        device_id: device.id.clone(),
        device_secret: new_secret,
        desktop_name: inner.desktop_name.clone(),
        ice_servers: ice_servers.clone(),
    });
    inner.update(|s| {
        s.phase = Phase::Connecting;
        s.device = Some(device.clone());
        s.pairing = None;
        s.error = None;
    });

    let end = signaling_loop(inner, id, tx, stream, &close, &ice_servers).await;

    inner.debug(|| format!("session {id} ended: {end:?}"));
    let still_current = inner.session.lock().unwrap().take_if(|s| s.id == id).is_some();
    *inner.phone_stats.lock().unwrap() = None;
    if still_current {
        match end {
            End::Lost => {
                inner.log("connection lost, waiting for phone to reconnect");
                inner.start_reconnect_window();
            }
            End::Bye => {
                inner.log("phone disconnected");
                inner.update(|s| {
                    s.phase = Phase::Disconnected;
                    s.device = None;
                    s.phone = None;
                });
            }
            End::Replaced | End::Rejected => {}
        }
    }
}

async fn next_message(inner: &Inner, stream: &mut SplitStream<WebSocket>) -> Option<PhoneMessage> {
    loop {
        match stream.next().await {
            Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
                Ok(m) => return Some(m),
                Err(e) => inner.debug(|| format!("ignored malformed message: {e}")),
            },
            Some(Ok(Message::Close(frame))) => {
                inner.debug(|| format!("socket closed by phone: {frame:?}"));
                return None;
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => {
                inner.debug(|| format!("socket error: {e}"));
                return None;
            }
            None => return None,
        }
    }
}

async fn signaling_loop(
    inner: &Arc<Inner>,
    id: u64,
    tx: &mpsc::UnboundedSender<DesktopMessage>,
    stream: &mut SplitStream<WebSocket>,
    close: &Notify,
    ice_servers: &[IceServer],
) -> End {
    let mut pc: Option<Arc<dyn PeerConnection>> = None;
    let end = loop {
        tokio::select! {
            _ = close.notified() => {
                let replaced = inner.session.lock().unwrap().as_ref().is_some_and(|s| s.id != id);
                break if replaced { End::Replaced } else { End::Bye };
            }
            msg = next_message(inner, stream) => match msg {
                None => break End::Lost,
                Some(PhoneMessage::Bye) => break End::Bye,
                Some(PhoneMessage::Hello { .. }) => break End::Rejected,
                Some(PhoneMessage::Offer { sdp }) => {
                    inner.debug(|| format!("offer received ({} bytes, codecs: {})", sdp.len(), codecs(&sdp)));
                    if let Some(old) = pc.take() {
                        inner.debug(|| "closing previous peer connection for renegotiation".into());
                        let _ = old.close().await;
                    }
                    match answer(inner, id, tx, sdp, ice_servers).await {
                        Ok(p) => pc = Some(p),
                        Err(e) => {
                            inner.log(format!("negotiation failed: {e}"));
                            inner.update(|s| s.error = Some(UserError { code: "negotiation_failed", detail: e.to_string() }));
                            break End::Lost;
                        }
                    }
                }
                Some(PhoneMessage::Ice { candidate }) => {
                    inner.debug(|| match &candidate {
                        Some(c) => format!("remote candidate: {}", c.candidate),
                        None => "remote candidates complete".into(),
                    });
                    if let (Some(pc), Some(c)) = (&pc, candidate) {
                        let init = RTCIceCandidateInit {
                            candidate: c.candidate,
                            sdp_mid: c.sdp_mid,
                            sdp_mline_index: c.sdp_mline_index,
                            ..Default::default()
                        };
                        if let Err(e) = pc.add_ice_candidate(init).await {
                            inner.debug(|| format!("bad remote candidate: {e}"));
                        }
                    }
                }
                Some(PhoneMessage::State { state }) => {
                    if inner.state.lock().unwrap().phone.as_ref() != Some(&state) {
                        inner.debug(|| format!("phone state: {state}"));
                    }
                    inner.update(|s| s.phone = Some(state));
                }
                Some(PhoneMessage::Stats { stats }) => *inner.phone_stats.lock().unwrap() = Some(stats),
            }
        }
    };
    if let Some(pc) = pc {
        let _ = pc.close().await;
    }
    end
}

fn rtc_config(ice_servers: &[IceServer]) -> RTCConfiguration {
    RTCConfigurationBuilder::new()
        .with_ice_servers(
            ice_servers
                .iter()
                .map(|s| RTCIceServer {
                    urls: s.urls.clone(),
                    username: s.username.clone().unwrap_or_default(),
                    credential: s.credential.clone().unwrap_or_default(),
                    ..Default::default()
                })
                .collect(),
        )
        .build()
}

fn is_current(inner: &Inner, id: u64) -> bool {
    inner.session.lock().unwrap().as_ref().is_some_and(|s| s.id == id)
}

async fn answer(
    inner: &Arc<Inner>,
    id: u64,
    tx: &mpsc::UnboundedSender<DesktopMessage>,
    sdp: String,
    ice_servers: &[IceServer],
) -> crate::Result<Arc<dyn PeerConnection>> {
    let (media, registry) = media_engine()?;
    let events = Events { inner: inner.clone(), id, tx: tx.clone() };
    let pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(rtc_config(ice_servers))
            .with_media_engine(media)
            .with_interceptor_registry(registry)
            .with_handler(Arc::new(events))
            // A wildcard binds one socket per non-loopback interface, like the old default.
            .with_udp_addrs(vec!["0.0.0.0:0"])
            .build()
            .await?,
    );

    pc.set_remote_description(RTCSessionDescription::offer(sdp)?).await?;
    let answer = pc.create_answer(None).await?;
    pc.set_local_description(answer).await?;
    let local = pc.local_description().await.ok_or("missing local description")?;
    inner.debug(|| format!("answer sent ({} bytes, codecs: {})", local.sdp.len(), codecs(&local.sdp)));
    let _ = tx.send(DesktopMessage::Answer { sdp: with_start_bitrate(&local.sdp) });
    Ok(pc)
}

struct Events {
    inner: Arc<Inner>,
    id: u64,
    tx: mpsc::UnboundedSender<DesktopMessage>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Events {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let c = event.candidate;
        self.inner.debug(|| format!("local candidate: {} {}:{} {}", c.protocol, c.address, c.port, c.typ));
        if let Ok(j) = c.to_json() {
            let candidate = IceCandidate { candidate: j.candidate, sdp_mid: j.sdp_mid, sdp_mline_index: j.sdp_mline_index };
            let _ = self.tx.send(DesktopMessage::Ice { candidate: Some(candidate) });
        }
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.inner.debug(|| "local candidates complete".into());
            let _ = self.tx.send(DesktopMessage::Ice { candidate: None });
        }
    }

    async fn on_ice_connection_state_change(&self, state: RTCIceConnectionState) {
        self.inner.debug(|| format!("ICE state: {state}"));
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let inner = &self.inner;
        inner.debug(|| format!("peer connection state: {state}"));
        if !is_current(inner, self.id) {
            return;
        }
        match state {
            RTCPeerConnectionState::Connected => {
                inner.log("media connected");
                inner.update(|s| {
                    s.phase = Phase::Connected;
                    s.error = None;
                });
            }
            RTCPeerConnectionState::Disconnected => inner.update(|s| s.phase = Phase::Reconnecting),
            RTCPeerConnectionState::Failed => {
                inner.log("media connection failed");
                inner.update(|s| {
                    s.phase = Phase::Reconnecting;
                    s.error = Some(UserError { code: "ice_failed", detail: String::new() });
                });
            }
            _ => {}
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        if track.kind().await == RtpCodecKind::Video {
            tokio::spawn(read_track(self.inner.clone(), self.id, track));
        }
    }
}

/// Lists the codec names a session description offers, such as "H264/90000".
fn codecs(sdp: &str) -> String {
    let mut names: Vec<&str> = sdp.lines().filter_map(|l| l.strip_prefix("a=rtpmap:")?.split_whitespace().nth(1)).collect();
    names.dedup();
    names.join(", ")
}

/// Tells the phone browser to start at good video quality instead of a slow, blurry start.
fn with_start_bitrate(sdp: &str) -> String {
    sdp.split_inclusive("\r\n")
        .map(|line| {
            if line.starts_with("a=fmtp:") && line.contains("profile-level-id") {
                format!("{};x-google-start-bitrate=2500\r\n", line.trim_end())
            } else {
                line.to_string()
            }
        })
        .collect()
}

async fn read_track(inner: Arc<Inner>, id: u64, track: Arc<dyn TrackRemote>) {
    inner.send_pipeline(pipeline::Msg::Reset);
    let ssrc = track.ssrcs().await.first().copied().unwrap_or_default();
    if let Some(codec) = track.codec(ssrc).await {
        inner.debug(|| format!("video track: {} ({}), ssrc {ssrc}", codec.mime_type, codec.sdp_fmtp_line));
    }

    // Asks the phone for a fresh full picture whenever needed, but never more often than the set gap.
    let pli_inner = inner.clone();
    let pli_track = track.clone();
    let pli = tokio::spawn(async move {
        let mut last = Instant::now() - Duration::from_secs(1);
        loop {
            let wait = KEYFRAME_REQUEST_GAP.saturating_sub(last.elapsed());
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            let pli = PictureLossIndication { sender_ssrc: 0, media_ssrc: ssrc };
            if pli_track.write_rtcp(vec![Box::new(pli)]).await.is_err() {
                break;
            }
            last = Instant::now();
            pli_inner.keyframe.notified().await;
        }
    });

    let mut builder = SampleBuilder::new(256, H264Packet::default(), 90_000);
    let mut last_seq: Option<u16> = None;
    let mut need_keyframe = true;
    while let Some(event) = track.poll().await {
        let TrackRemoteEvent::OnRtpPacket(packet) = event else { continue };
        if !is_current(&inner, id) {
            break;
        }
        let seq = packet.header.sequence_number;
        inner.net.bytes.fetch_add(packet.payload.len() as u64, Ordering::Relaxed);
        inner.net.packets.fetch_add(1, Ordering::Relaxed);
        // Counts skipped packet numbers as lost and ignores late packets that arrive out of order.
        match last_seq {
            Some(prev) => {
                let gap = seq.wrapping_sub(prev);
                if gap < 0x8000 {
                    if gap > 1 {
                        inner.net.lost.fetch_add(gap as u64 - 1, Ordering::Relaxed);
                    }
                    last_seq = Some(seq);
                }
            }
            None => last_seq = Some(seq),
        }
        builder.push(Instant::now(), packet);
        while let Some(sample) = builder.pop(Instant::now()) {
            if sample.prev_dropped_packets > 0 {
                inner.debug(|| format!("{} packet(s) dropped before frame, requesting keyframe", sample.prev_dropped_packets));
                need_keyframe = true;
                inner.keyframe.notify_one();
            }
            let key = is_keyframe(&sample.data);
            let overloaded = inner.counters.pending.load(Ordering::Relaxed) > MAX_PENDING_FRAMES;
            if !key && (need_keyframe || overloaded) {
                inner.counters.dropped.fetch_add(1, Ordering::Relaxed);
                if overloaded {
                    if !need_keyframe {
                        inner.debug(|| "decoder falling behind, skipping to next keyframe".into());
                    }
                    need_keyframe = true;
                    inner.keyframe.notify_one();
                }
                continue;
            }
            if need_keyframe {
                inner.debug(|| format!("keyframe received ({} bytes)", sample.data.len()));
            }
            need_keyframe = false;
            inner.counters.pending.fetch_add(1, Ordering::Relaxed);
            inner.send_pipeline(pipeline::Msg::Frame(sample.data.to_vec()));
        }
    }
    pli.abort();
    inner.debug(|| format!("track {ssrc} stopped"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_sdp_codecs() {
        let sdp = "v=0\r\na=rtpmap:102 H264/90000\r\na=rtpmap:108 H264/90000\r\na=rtpmap:111 opus/48000/2\r\n";
        assert_eq!(super::codecs(sdp), "H264/90000, opus/48000/2");
    }

    #[test]
    fn start_bitrate_is_added_to_h264_fmtp_only() {
        let sdp = "v=0\r\na=fmtp:102 packetization-mode=1;profile-level-id=42e01f\r\na=fmtp:111 minptime=10\r\n";
        let out = super::with_start_bitrate(sdp);
        assert!(out.contains("profile-level-id=42e01f;x-google-start-bitrate=2500\r\n"));
        assert!(out.contains("a=fmtp:111 minptime=10\r\n"));
    }
}
