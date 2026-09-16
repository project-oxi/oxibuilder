import { BookOpen } from "lucide-react";

import { pickVariant, useOptimizedImage } from "../../shared/useOptimizedImage";

const EAGER_COUNT = 10;

export interface BookCardData {
  id: number;
  title: string;
  author: string | null;
  cover_image_url: string | null;
  category?: string | null;
  published_year?: string | null;
}

type Pick = (ko?: string | null, en?: string | null) => string;

/**
 * Cover-first grid tile in the legacy-collection language: 2:3 cover,
 * hairline footer with title / author·year / category tag.
 */
export function BookCard({
  book,
  index,
  pick: _pick,
}: {
  book: BookCardData;
  index?: number;
  pick?: Pick;
}) {
  const optimized = useOptimizedImage(book.cover_image_url);
  const variant = optimized ? pickVariant(optimized) : null;
  const eager = index != null && index < EAGER_COUNT;
  const meta = [book.author, book.published_year].filter(Boolean).join(" · ");

  const imgCls =
    "h-full w-full object-cover transition-transform duration-300 group-hover:scale-105 motion-reduce:transform-none";

  return (
    <article className="group overflow-hidden rounded-lg border border-line transition-colors duration-200 hover:border-line-strong">
      <div className="aspect-[2/3] overflow-hidden bg-surface">
        {variant ? (
          <img
            src={variant.url}
            srcSet={optimized!.srcset.map((s) => `${s.url} ${s.w}w`).join(", ")}
            sizes="(min-width: 1024px) 18vw, (min-width: 640px) 24vw, 46vw"
            width={optimized!.width}
            height={optimized!.height}
            alt=""
            loading={eager ? "eager" : "lazy"}
            fetchPriority={eager ? "high" : "auto"}
            decoding="async"
            className={imgCls}
          />
        ) : book.cover_image_url ? (
          <img
            src={book.cover_image_url}
            alt=""
            loading={eager ? "eager" : "lazy"}
            fetchPriority={eager ? "high" : "auto"}
            className={imgCls}
          />
        ) : (
          <div className="flex h-full items-center justify-center text-subtle">
            <BookOpen className="size-6" />
          </div>
        )}
      </div>
      <div className="space-y-1 border-t border-line p-3">
        <h3 className="truncate text-sm font-medium text-foreground">{book.title}</h3>
        {meta && <p className="truncate text-xs text-subtle">{meta}</p>}
        {book.category && (
          <div className="flex pt-0.5">
            <span className="rounded-xs border border-line px-1.5 py-0.5 text-[10px] text-subtle">
              {book.category}
            </span>
          </div>
        )}
      </div>
    </article>
  );
}
