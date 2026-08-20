export type Tool =
  | "select"
  | "erase"
  | "highlight"
  | "underline"
  | "strikeOut"
  | "note"
  | "ink"
  | "redact"
  | "editText"
  | "moveText"
  | "moveImage"
  | "addTextField"
  | "addCheckbox"
  | "signature";

interface ToolDef {
  id: Tool;
  label: string;
  /** Lucide glyph name (static/icons/<icon>.svg) shown in the tool rail. */
  icon: string;
  /** Rail tools render as small groups with a gap between them — this is
   * true on the first tool of a new group, so +page.svelte can space
   * them without a second parallel list to keep in sync. */
  startsGroup?: boolean;
}

export const TOOLS: ToolDef[] = [
  { id: "select", label: "Select", icon: "mouse-pointer-2", startsGroup: true },
  { id: "erase", label: "Erase", icon: "eraser" },
  { id: "highlight", label: "Highlight", icon: "highlighter", startsGroup: true },
  { id: "underline", label: "Underline", icon: "underline" },
  { id: "strikeOut", label: "Strikeout", icon: "strikethrough" },
  { id: "note", label: "Note", icon: "message-square", startsGroup: true },
  { id: "ink", label: "Draw", icon: "pen-line" },
  { id: "redact", label: "Redact", icon: "square-dashed", startsGroup: true },
  { id: "editText", label: "Edit text", icon: "type", startsGroup: true },
  { id: "moveText", label: "Move text", icon: "move" },
  { id: "moveImage", label: "Move image", icon: "image" },
  { id: "addTextField", label: "Add text field", icon: "text-cursor-input", startsGroup: true },
  { id: "addCheckbox", label: "Add checkbox", icon: "square-check" },
  { id: "signature", label: "Signature", icon: "signature", startsGroup: true },
];
