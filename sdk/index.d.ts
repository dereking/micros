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

declare function state<T extends Scalar>(initial: T): State<T>;
declare function bind<T extends Scalar>(read: () => T): Binding<T>;

declare const ui: {
  column(children: UiNode[]): UiNode;
  text(value: string | Binding<string>): UiNode;
  button(label: string, options: { onClick: () => void }): UiNode;
  mount(root: UiNode): void;
};
