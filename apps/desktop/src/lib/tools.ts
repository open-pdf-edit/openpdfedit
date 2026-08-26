export type Tool =
  | "select"
  | "erase"
  | "highlight"
  | "underline"
  | "strikeOut"
  | "note"
  | "ink"
  | "rectangle"
  | "ellipse"
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
}

/**
 * The rail's tools, in the groups they are shown in.
 *
 * Sixteen glyphs in a column, distinguishable only by hovering each one
 * in turn, is a memory test rather than a toolbar — and the two most
 * consequential tools in it (Redact, which destroys content, and Erase,
 * which deletes annotations) look like a dotted rectangle and a small
 * wedge. The rail shows names, and the names are grouped, so a tool can
 * be found by what it is for rather than by recognising its picture.
 *
 * The group each tool is in is a claim about what it does. Redact sits
 * with the content edits rather than with the markup tools because it
 * removes what is underneath rather than drawing over it — the same
 * reason it is the one tool here that cannot be undone by deleting an
 * annotation afterwards.
 */
export interface ToolGroup {
  /** Shown above the group on a desktop; a divider stands in for it on a
   * phone, where the rail is a horizontal strip with no room for
   * headings. */
  name: string;
  tools: ToolDef[];
}

export const TOOL_GROUPS: ToolGroup[] = [
  {
    name: "Select",
    tools: [
      { id: "select", label: "Select", icon: "mouse-pointer-2" },
      { id: "erase", label: "Erase", icon: "eraser" },
    ],
  },
  {
    name: "Mark up",
    tools: [
      { id: "highlight", label: "Highlight", icon: "highlighter" },
      { id: "underline", label: "Underline", icon: "underline" },
      { id: "strikeOut", label: "Strikeout", icon: "strikethrough" },
      { id: "note", label: "Note", icon: "message-square" },
    ],
  },
  {
    name: "Draw",
    tools: [
      { id: "ink", label: "Draw", icon: "pen-line" },
      { id: "rectangle", label: "Rectangle", icon: "square" },
      { id: "ellipse", label: "Ellipse", icon: "circle" },
    ],
  },
  {
    name: "Edit content",
    tools: [
      { id: "editText", label: "Edit text", icon: "type" },
      { id: "moveText", label: "Move text", icon: "move" },
      { id: "moveImage", label: "Move image", icon: "image" },
      { id: "redact", label: "Redact", icon: "square-dashed" },
    ],
  },
  {
    name: "Fill & sign",
    tools: [
      { id: "addTextField", label: "Add text field", icon: "text-cursor-input" },
      { id: "addCheckbox", label: "Add checkbox", icon: "square-check" },
      { id: "signature", label: "Signature", icon: "signature" },
    ],
  },
];

/** Every tool, ungrouped, in rail order. */
export const TOOLS: ToolDef[] = TOOL_GROUPS.flatMap((group) => group.tools);
