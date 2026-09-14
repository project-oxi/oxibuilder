import { useQuery } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { Link, useParams } from "react-router";

import { fetchBlogPost, fetchBlogPosts, fetchBlogSeries } from "../../shared/api";
import { useLanguage } from "../../shared/language";
import { Button } from "../../shared/ui/button";
import { BlogPostView } from "./BlogPostView";

export function BlogPostPage() {
  const { slug = "" } = useParams();
  const { lang } = useLanguage();
  const { data: post, isLoading, error } = useQuery({
    queryKey: ["blog", slug],
    queryFn: () => fetchBlogPost(slug),
    enabled: !!slug,
  });
  // 번역 링크와 시리즈 맥락은 컬렉션에서 해소한다 (static/live 공용 캐시).
  const { data: allPosts } = useQuery({
    queryKey: ["blog", "list", lang],
    queryFn: () => fetchBlogPosts(),
    enabled: !!post,
  });
  const { data: series } = useQuery({
    queryKey: ["blog", "series", post?.series_id],
    queryFn: () => fetchBlogSeries(),
    enabled: !!post?.series_id,
  });

  if (isLoading) return <p className="text-subtle">…</p>;
  if (error || !post) {
    return (
      <div className="space-y-4">
        <Button variant="ghost" size="sm" asChild>
          <Link to="/blog">
            <ArrowLeft />
            {lang === "ko" ? "블로그" : "Blog"}
          </Link>
        </Button>
        <p className="text-subtle">
          {lang === "ko" ? "게시물을 찾을 수 없습니다." : "Post not found."}
        </p>
      </div>
    );
  }

  const translation =
    post.translation_group_id != null
      ? allPosts?.find(
          (p) =>
            p.slug !== post.slug &&
            p.translation_group_id === post.translation_group_id &&
            p.lang !== post.lang,
        )
      : undefined;
  const mySeries = series?.find((s) => s.id === post.series_id);
  const seriesPosts = mySeries
    ? allPosts
        ?.filter((p) => p.series_id === mySeries.id)
        .sort(
          (a, b) =>
            (a.series_order ?? Number.MAX_SAFE_INTEGER) -
            (b.series_order ?? Number.MAX_SAFE_INTEGER),
        )
    : undefined;
  const idx = seriesPosts?.findIndex((p) => p.slug === post.slug) ?? -1;
  const prev = idx > 0 ? seriesPosts?.[idx - 1] : undefined;
  const next =
    idx >= 0 && seriesPosts && idx < seriesPosts.length - 1
      ? seriesPosts[idx + 1]
      : undefined;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <Button variant="ghost" size="sm" asChild className="-ml-2">
          <Link to="/blog">
            <ArrowLeft />
            {lang === "ko" ? "블로그" : "Blog"}
          </Link>
        </Button>
        {translation && (
          <Link
            to={`/blog/${translation.slug}`}
            className="text-sm text-subtle underline-offset-4 hover:text-foreground hover:underline"
          >
            {translation.lang === "ko" ? "한국어로 읽기" : "Read in English"}
          </Link>
        )}
      </div>
      {mySeries && (
        <div className="rounded-lg border border-line bg-surface px-4 py-3 text-sm">
          <Link
            to={`/blog/series/${mySeries.slug}`}
            className="font-medium text-foreground underline-offset-4 hover:underline"
          >
            {lang === "ko" ? "시리즈" : "Series"}: {mySeries.title}
          </Link>
          {typeof post.series_order === "number" && (
            <span className="ml-2 text-subtle">
              {lang === "ko" ? `${post.series_order}화` : `#${post.series_order}`}
            </span>
          )}
          {(prev || next) && (
            <div className="mt-2 flex justify-between gap-4 text-xs text-subtle">
              {prev ? (
                <Link to={`/blog/${prev.slug}`} className="hover:text-foreground">
                  ← {prev.title}
                </Link>
              ) : (
                <span />
              )}
              {next ? (
                <Link to={`/blog/${next.slug}`} className="hover:text-foreground">
                  {next.title} →
                </Link>
              ) : (
                <span />
              )}
            </div>
          )}
        </div>
      )}
      <BlogPostView post={post} language={lang} />
    </div>
  );
}
