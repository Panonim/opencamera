import { create } from "zustand";

export type ToastTone = "success" | "info" | "error";
export interface Toast {
  id: number;
  tone: ToastTone;
  title: string;
  description?: string;
  leaving?: boolean;
}

const LIFETIME_MS = 6000;
// Matches the exit animation in Toaster.
const EXIT_MS = 200;
let nextId = 0;

export const useToasts = create<{ toasts: Toast[] }>(() => ({ toasts: [] }));

export const dismissToast = (id: number) => useToasts.setState((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));

function expireToast(id: number) {
  useToasts.setState((s) => ({ toasts: s.toasts.map((t) => (t.id === id ? { ...t, leaving: true } : t)) }));
  setTimeout(() => dismissToast(id), EXIT_MS);
}

export function toast(tone: ToastTone, title: string, description?: string) {
  const id = nextId++;
  useToasts.setState((s) => ({ toasts: [...s.toasts, { id, tone, title, description }] }));
  setTimeout(() => expireToast(id), LIFETIME_MS);
}
