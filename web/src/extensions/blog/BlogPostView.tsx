import { Calendar } from "lucide-react";

import { Markdown } from "../../shared/Markdown";
import { Badge } from "../../shared/ui/badge";

export interface BlogPostData {
  title: string;
  body: string;
  lang: "ko" | "en";
  tags: string[];
  published_at: string | null;
  created_at: string;
  category?: string | null;
}

interface BlogPostViewProps {
  post: BlogPostData;
  language: "ko" | "en";
}

/**
 * Article layout — the body renders directly on the canvas in a centered
 * reading column (max-w-3xl), never boxed. Typographic scale comes from
 * `.markdown-lg` + the global markdown rules.
 */
export function BlogPostView({ post, language: _language }: BlogPostViewProps) {
  const date = (post.published_at ?? post.created_at).slice(0, 10);
  return (
    <article className="mx-auto max-w-3xl">
      <header className="mb-10 space-y-4">
        <h1 className="font-serif text-4xl font-bold leading-[1.25] tracking-tight text-foreground">
          {post.title}
        </h1>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-sm text-subtle">
          <span className="inline-flex items-center gap-1.5 tabular-nums">
            <Calendar className="size-3.5" />
            <time>{date}</time>
          </span>
          {post.category && (
            <span className="text-xs border border-line rounded-full px-2.5 py-0.5">
              {post.category}
            </span>
          )}
          <Badge variant="outline">{post.lang === "ko" ? "한국어" : "English"}</Badge>
          {post.tags.map((t) => (
            <Badge key={t} variant="secondary">
              {t}
            </Badge>
          ))}
        </div>
      </header>
      <div className="markdown-lg">
        <Markdown source={post.body || "*No content*"} />
      </div>
    </article>
  );
}
