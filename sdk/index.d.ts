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

interface UiTextStyle {
  font: "uiSans";
  size: 12 | 14 | 18 | 24 | 32;
  weight: "regular";
  lineHeight: number;
}

declare function state<T extends Scalar>(initial: T): State<T>;
declare function bind<T extends Scalar>(read: () => T): Binding<T>;

declare const ui: {
  column(children: UiNode[]): UiNode;
  text(value: string | Binding<string>, style?: UiTextStyle): UiNode;
  button(label: string, options: { onClick: () => void; textStyle?: UiTextStyle }): UiNode;
  mount(root: UiNode): void;
};
