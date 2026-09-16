import { Link } from "react-router";

export interface BlogPostCardData {
  slug: string;
  title: string;
  tags: string[];
  published_at: string | null;
  category: string | null;
}

/**
 * Editorial index row. The parent <ul> owns the hairline dividers
 * (`divide-y divide-line border-y border-line`); this renders one entry:
 * title + category on the baseline row, date + tags in small mono-spaced text.
 */
export function BlogPostCard({ post }: { post: BlogPostCardData }) {
  const date = post.published_at?.slice(0, 10) ?? null;
  return (
    <li className="group">
      <Link to={`/blog/${post.slug}`} className="block py-5 text-foreground no-underline">
        <div className="flex items-baseline justify-between gap-4">
          <h2 className="font-serif text-xl font-semibold tracking-tight transition-colors duration-200 group-hover:text-primary">
            {post.title}
          </h2>
          {post.category && (
            <span className="shrink-0 text-xs text-subtle">{post.category}</span>
          )}
        </div>
        <p className="mt-1.5 text-xs tabular-nums text-subtle">
          {date && <time>{date}</time>}
          {post.tags.length > 0 && (
            <span className={date ? "ml-3" : undefined}>#{post.tags.join(" #")}</span>
          )}
        </p>
      </Link>
    </li>
  );
}
