import { useLayoutEffect, useRef, useState, type MutableRefObject } from 'react';

export interface ElementSize {
  width: number;
  height: number;
}

export function useElementSize<T extends HTMLElement>(): [MutableRefObject<T | null>, ElementSize] {
  const [size, setSize] = useState<ElementSize>({ width: 0, height: 0 });
  const ref = useRef<T | null>(null);

  useLayoutEffect(() => {
    if (!ref.current) {
      return;
    }

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) {
        return;
      }

      const { width, height } = entry.contentRect;
      setSize({
        width: Math.round(width),
        height: Math.round(height),
      });
    });

    observer.observe(ref.current);
    return () => observer.disconnect();
  }, []);

  return [ref, size];
}
