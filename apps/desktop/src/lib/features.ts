/**
 * The M1 AI setup screens (Settings → AI models and usage, question (b) on the course's policy
 * tab). They run against the mock until src-tauri has the AI commands (plan M1, alpha.2), so a
 * real build (alpha.1) doesn't show screens whose commands don't exist yet. Set to true in the
 * commit that wires the commands.
 */
export const AI_SETUP_ENABLED = import.meta.env.VITE_API === "mock";
