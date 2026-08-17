import type { Config } from "tailwindcss";

const config: Config = {
  content: ["./src/**/*.{js,ts,jsx,tsx,mdx}"],
  theme: {
    extend: {
      colors: {
        ink: "#050505",
        panel: "#0e0e10",
        raised: "#16161a",
        line: "#26262c",
        mute: "#8b8b96",
        accent: "#15dae3",
        ok: "#3dd68c",
        warn: "#e6b450",
        bad: "#f07178",
      },
      fontFamily: {
        sans: ["var(--font-geist-sans)", "system-ui", "sans-serif"],
        mono: ["var(--font-geist-mono)", "ui-monospace", "monospace"],
      },
      boxShadow: {
        glow: "0 0 0 1px rgba(21,218,227,0.25), 0 8px 32px rgba(21,218,227,0.08)",
      },
    },
  },
  plugins: [],
};
export default config;
