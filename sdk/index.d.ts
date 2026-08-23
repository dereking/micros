type Scalar = number | string | boolean | null;

interface State<T extends Scalar> {
  value: T;
}

interface Binding<T extends Scalar> {
  readonly __binding: T;
}

interface UiNode {
  readonly __node: unique symbol;
}

type UiTextStyle =
  | { font: "uiSans"; size: 12; weight: "regular"; lineHeight: 14 }
  | { font: "uiSans"; size: 14; weight: "regular"; lineHeight: 18 }
  | { font: "uiSans"; size: 18; weight: "regular"; lineHeight: 24 }
  | { font: "uiSans"; size: 24; weight: "regular"; lineHeight: 32 }
  | { font: "uiSans"; size: 32; weight: "regular"; lineHeight: 40 };

declare function state<T extends Scalar>(initial: T): State<T>;
declare function bind<T extends Scalar>(read: () => T): Binding<T>;

declare const ui: {
  column(children: UiNode[]): UiNode;
  row(children: UiNode[]): UiNode;
  text(value: string | Binding<string>, style?: UiTextStyle): UiNode;
  button(label: string, options: { onClick: () => void; textStyle?: UiTextStyle }): UiNode;
  progress(value: number | Binding<number>): UiNode;
  switch(value: Binding<boolean>, options?: { onToggle?: () => void }): UiNode;
  input(
    value: string | Binding<string>,
    options?: { placeholder?: string; onChange?: (text: string) => void },
  ): UiNode;
  slider(
    value: number | Binding<number>,
    options?: { min?: number; max?: number; onChange?: (value: number) => void },
  ): UiNode;
  checkbox(
    label: string,
    checked: boolean | Binding<boolean>,
    options?: { onChange?: (value: boolean) => void },
  ): UiNode;
  dropdown(
    options: string[],
    index: number | Binding<number>,
    opts?: { onChange?: (index: number) => void },
  ): UiNode;
  roller(
    options: string[],
    index: number | Binding<number>,
    opts?: { onChange?: (index: number) => void },
  ): UiNode;
  led(on: boolean | Binding<boolean>): UiNode;
  spinner(active: boolean | Binding<boolean>): UiNode;
  scale(value: number | Binding<number>, options?: { min?: number; max?: number }): UiNode;
  list(items: { text: string; onClick: () => void }[]): UiNode;
  tabview(tabs: { title: string; content: UiNode }[]): UiNode;
  place(
    widget: UiNode,
    layout: {
      /** Base left position (used when the left edge is not anchored). */
      left?: number;
      /** Base top position (used when the top edge is not anchored). */
      top?: number;
      /** Base width (used when the horizontal axis is not stretched by anchors). */
      width?: number;
      /** Base height (used when the vertical axis is not stretched by anchors). */
      height?: number;
      /** Edge anchors: an offset pins that edge to the parent's edge, taking
       * priority over the base position; both opposite edges set stretches. */
      anchor?: {
        left?: number;
        top?: number;
        right?: number;
        bottom?: number;
      };
      /**
       * Dock the widget to an edge (Delphi Align) — seeds the default anchor
       * combo; explicit `anchor` edges override per edge.
       * - "top": `{ left: 0, right: 0, top: 0 }`
       * - "bottom": `{ left: 0, right: 0, bottom: 0 }`
       * - "left": `{ left: 0, top: 0, bottom: 0 }`
       * - "right": `{ right: 0, top: 0, bottom: 0 }`
       * - "client": `{ left: 0, top: 0, right: 0, bottom: 0 }`
       * - "none": no anchors (base ltwh positioning)
       */
      align?: "top" | "bottom" | "left" | "right" | "client" | "none";
    },
  ): UiNode;
  mount(root: UiNode): void;
};
