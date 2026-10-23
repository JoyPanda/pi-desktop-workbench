"use client";

import Image from "next/image";
import { useEffect, useId, useRef } from "react";

import { useLightbox } from "@/components/lightbox";

/**
 * A product screenshot. The PNGs already carry their own window chrome and a
 * transparent gutter, so the image is rendered bare — no card frame, border,
 * padding, or shadow around it.
 */
export function Shot({
  src,
  srcLight,
  alt,
  width = 3164,
  height = 2068,
  priority = false,
  sizes = "(max-width: 768px) 100vw, (max-width: 1280px) 92vw, 1180px",
  className = "",
}: {
  src: string;
  srcLight?: string;
  alt: string;
  width?: number;
  height?: number;
  priority?: boolean;
  sizes?: string;
  className?: string;
}) {
  const { register, open } = useLightbox();
  const id = useId();
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const el = buttonRef.current;
    if (!el) return;
    return register({ id, src, srcLight, alt, width, height }, el);
  }, [register, id, src, srcLight, alt, width, height]);

  return (
    <div className={className}>
      <button
        ref={buttonRef}
        type="button"
        onClick={() => open(id)}
        title="Click to enlarge"
        className="block w-full cursor-zoom-in outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <Image
          src={src}
          alt={alt}
          width={width}
          height={height}
          priority={priority}
          sizes={sizes}
          className={`h-auto w-full${srcLight ? " light:hidden" : ""}`}
        />
        {srcLight ? (
          <Image
            src={srcLight}
            alt={alt}
            width={width}
            height={height}
            priority={priority}
            sizes={sizes}
            className="hidden h-auto w-full light:block"
          />
        ) : null}
      </button>
    </div>
  );
}
