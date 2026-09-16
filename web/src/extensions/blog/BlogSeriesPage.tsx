import { useQuery } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { Link, useParams } from "react-router";

import { fetchBlogSeriesBySlug } from "../../shared/api";
import { useLanguage } from "../../shared/language";
import { Button } from "../../shared/ui/button";

export function BlogSeriesPage() {
  const { slug = "" } = useParams();
  const { lang } = useLanguage();
  const { data: series, isLoading, error } = useQuery({
    queryKey: ["blog", "series-detail", slug],
    queryFn: () => fetchBlogSeriesBySlug(slug),
    enabled: !!slug,
  });

  if (isLoading) return <p className="text-subtle">…</p>;
  if (error || !series) {
    return (
      <div className="space-y-4">
        <Button variant="ghost" size="sm" asChild>
          <Link to="/blog">
            <ArrowLeft />
            {lang === "ko" ? "블로그" : "Blog"}
          </Link>
        </Button>
        <p className="text-subtle">
          {lang === "ko" ? "시리즈를 찾을 수 없습니다." : "Series not found."}
        </p>
      </div>
    );
  }

  return (
    <article className="space-y-8">
      <Button variant="ghost" size="sm" asChild className="-ml-2">
        <Link to="/blog">
          <ArrowLeft />
          {lang === "ko" ? "블로그" : "Blog"}
        </Link>
      </Button>
      <header className="space-y-2">
        <p className="text-xs font-medium uppercase tracking-widest text-subtle">
          {lang === "ko" ? "시리즈" : "Series"}
        </p>
        <h1 className="font-serif text-4xl font-bold tracking-tight text-foreground">
          {series.title}
        </h1>
        {series.description && <p className="text-muted">{series.description}</p>}
        <p className="text-sm tabular-nums text-subtle">
          {lang === "ko" ? `${series.posts.length}편` : `${series.posts.length} posts`}
        </p>
      </header>
      <ol className="divide-y divide-line border-y border-line">
        {series.posts.map((p, i) => (
          <li key={p.slug} className="group">
            <Link
              to={`/blog/${p.slug}`}
              className="flex items-baseline gap-4 py-5 text-foreground no-underline"
            >
              <span className="w-6 shrink-0 text-sm tabular-nums text-subtle">
                {typeof p.series_order === "number" ? p.series_order : i + 1}
              </span>
              <span className="min-w-0">
                <h2 className="font-serif text-xl font-semibold tracking-tight transition-colors duration-200 group-hover:text-primary">
                  {p.title}
                </h2>
                <span className="mt-1 block text-xs tabular-nums text-subtle">
                  {(p.published_at ?? p.created_at).slice(0, 10)}
                </span>
              </span>
            </Link>
          </li>
        ))}
      </ol>
    </article>
  );
}
