// Provisional until backend-1's feat/course-ai (cc5427f) merges into the lane: AiLabel.on_device
// (calendar design §7.12). Opening a material's local file is `material_local_file` behind
// frontend-1's open_material / reveal_material commands (client.ts). Delete this file when
// `pnpm gen:types` brings the field.

declare module "../generated" {
  interface AiLabel {
    /** The model ran on this computer (false or absent for older rows). */
    on_device?: boolean | null;
  }
}

export {};
