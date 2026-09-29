export type ErrorCorrection = "low" | "medium" | "quartile" | "high";
export type ModuleShape = "square" | "dot";

export interface RendererOptions {
  /** Background color as #RGB, #RGBA, #RRGGBB, or #RRGGBBAA. */
  background?: string;
  /** Foreground color as #RGB, #RGBA, #RRGGBB, or #RRGGBBAA. */
  foreground?: string;
  /** Number of light modules surrounding the QR symbol. Defaults to 4. */
  margin?: number;
  /** Styling for data modules. Functional patterns always remain square. */
  moduleShape?: ModuleShape;
  /** Error-correction strength. Defaults to medium. */
  errorCorrection?: ErrorCorrection;
  /** Trusted standalone SVG used only for SVG output. */
  logoSvg?: string;
  /** Non-interlaced 8-bit RGB/RGBA PNG used only for PNG output. */
  logoPng?: Buffer;
  /** Maximum logo width/height as a fraction of output size. Defaults to 1/3. */
  logoScale?: number;
  /** Logo backing padding measured in QR modules. Defaults to 0.35. */
  logoPadding?: number;
  /** Logo backing color; defaults to the QR background. */
  logoBackground?: string;
  /** Optional vector outline color used only for SVG logos. */
  svgLogoOutlineColor?: string;
  /** SVG outline width in the logo viewBox's coordinate units. Defaults to 0. */
  svgLogoOutlineWidth?: number;
}

export interface OutputOptions {
  /** Exact output width and height in pixels. Defaults to 512. */
  size?: number;
}

/**
 * Immutable native renderer. Parse and validate a style once, then reuse the
 * instance for every payload.
 */
export class QrRenderer {
  constructor(options?: RendererOptions);

  /** Render an SVG document synchronously. */
  svg(text: string, options?: OutputOptions): string;

  /** Render a PNG synchronously and return its encoded bytes. */
  png(text: string, options?: OutputOptions): Buffer;
}

declare const _default: { QrRenderer: typeof QrRenderer };
export default _default;
