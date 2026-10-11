import { Geist_Mono, Sora } from "next/font/google";

/**
 * Shared font instances. Loaded once here and consumed by both the root layout
 * and the standalone `global-error` document (which replaces the layout, so it
 * cannot inherit the root's font variables).
 */
export const sora = Sora({
  subsets: ["latin"],
  variable: "--font-sora",
  display: "swap",
});

export const geistMono = Geist_Mono({
  subsets: ["latin"],
  variable: "--font-geist-mono",
  display: "swap",
});

/** The `className` that exposes both font variables. */
export const FONT_CLASSES = `${sora.variable} ${geistMono.variable}`;
