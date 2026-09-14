import { useQuery } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { Link, useParams } from "react-router";

import { fetchBlogSeriesBySlug } from "../../shared/api";
import { useLanguage } from "../../shared/language";
import { Button } from "../../shared/ui/button";
import { Card, CardContent } from "../../shared/ui/card";

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
    <article className="space-y-6">
      <Button variant="ghost" size="sm" asChild className="-ml-2">
        <Link to="/blog">
          <ArrowLeft />
          {lang === "ko" ? "블로그" : "Blog"}
        </Link>
      </Button>
      <header className="space-y-2">
        <p className="text-sm text-subtle">{lang === "ko" ? "시리즈" : "Series"}</p>
        <h1 className="font-serif text-3xl font-semibold tracking-tight text-foreground">
          {series.title}
        </h1>
        {series.description && <p className="text-subtle">{series.description}</p>}
        <p className="text-sm text-subtle">
          {lang === "ko" ? `${series.posts.length}편` : `${series.posts.length} posts`}
        </p>
      </header>
      <ol className="space-y-3">
        {series.posts.map((p, i) => (
          <li key={p.slug}>
            <Card className="transition-[border-color,box-shadow] duration-200 hover:border-primary/40 hover:shadow-md">
              <Link to={`/blog/${p.slug}`} className="block p-5 text-foreground no-underline">
                <div className="flex items-baseline gap-3">
                  <span className="font-serif text-lg text-subtle">
                    {typeof p.series_order === "number" ? p.series_order : i + 1}.
                  </span>
                  <h2 className="font-serif text-xl font-semibold tracking-tight">{p.title}</h2>
                </div>
                <p className="mt-1 text-xs text-subtle">
                  {(p.published_at ?? p.created_at).slice(0, 10)}
                </p>
              </Link>
            </Card>
          </li>
        ))}
      </ol>
    </article>
  );
}
