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
  mount(root: UiNode): void;
};
