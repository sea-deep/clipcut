/// <reference types="vite/client" />

// Vitest in-source testing support
interface ImportMeta {
  readonly vitest: typeof import("vitest") | undefined;
}
