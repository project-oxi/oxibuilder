import { Link } from "react-router";

import { Card } from "../../shared/ui/card";

export interface BlogPostCardData {
  slug: string;
  title: string;
  tags: string[];
  published_at: string | null;
  category: string | null;
}

export function BlogPostCard({ post }: { post: BlogPostCardData }) {
  return (
    <Card className="transition-[border-color,box-shadow] duration-200 hover:border-primary/40 hover:shadow-md">
      <Link to={`/blog/${post.slug}`} className="block p-5 text-foreground no-underline">
        <div className="flex items-start justify-between gap-3">
          <h2 className="font-serif text-xl font-semibold tracking-tight">{post.title}</h2>
          {post.category && (
            <span className="shrink-0 rounded-full border border-line px-2 py-0.5 text-xs text-muted">
              {post.category}
            </span>
          )}
        </div>
        {post.tags.length > 0 && (
          <p className="mt-2 text-xs text-muted">#{post.tags.join(" #")}</p>
        )}
      </Link>
    </Card>
  );
}
