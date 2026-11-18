// The renderer uses this subset of the React 19 API.
declare module 'react' {
  const element: unique symbol;
  export const Fragment: unique symbol;
  export interface ReactElement {
    readonly [element]: true;
  }
  export type ReactNode = ReactElement | string | null;
  export function createElement<P extends object>(
    type: (props: P) => ReactNode,
    props: P,
    ...children: ReactNode[]
  ): ReactElement;
  export function createElement(
    type: string | typeof Fragment | (() => ReactNode),
    props?: Record<string, unknown> | null,
    ...children: ReactNode[]
  ): ReactElement;
  export function useState<T>(initial: T): [T, (value: T) => void];
  export function useSyncExternalStore<T>(
    subscribe: (listener: () => void) => () => void,
    read: () => T,
  ): T;
}

declare module 'react-dom/client' {
  import type { ReactNode } from 'react';
  export function createRoot(target: Element): { render(value: ReactNode): void };
}
