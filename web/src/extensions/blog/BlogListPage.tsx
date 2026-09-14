import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useSearchParams } from "react-router";

import { fetchBlogPosts } from "../../shared/api";
import type { BlogPost } from "../../shared/api";
import { useLanguage } from "../../shared/language";
import { EmptyState, EmptyStateDescription, EmptyStateIcon, EmptyStateTitle } from "../../shared/ui/empty-state";
import { PageTitle } from "../../shared/ui/page-header";
import { NotebookPen } from "lucide-react";
import { BlogPostCard } from "./BlogPostCard";

/**
 * 언어별 뷰: 각 translation group에서 현재 언어 글을 하나만 보여준다.
 * 그룹에 현재 언어 글이 없으면(미번역) 그룹의 대표 글로 폴백 — 어떤 언어로
 * 봐도 모든 글이 정확히 한 번씩 나타난다. 그룹 없는 글은 그대로 통과.
 */
function pickPerLanguage(posts: BlogPost[], lang: "ko" | "en"): BlogPost[] {
  const groups = new Map<number, BlogPost[]>();
  const picked: BlogPost[] = [];
  for (const p of posts) {
    if (p.translation_group_id == null) {
      picked.push(p);
    } else {
      const group = groups.get(p.translation_group_id);
      if (group) group.push(p);
      else groups.set(p.translation_group_id, [p]);
    }
  }
  for (const group of groups.values()) {
    picked.push(group.find((p) => p.lang === lang) ?? group[0]);
  }
  return picked.sort(
    (a, b) => (b.published_at ?? b.created_at).localeCompare(a.published_at ?? a.created_at),
  );
}

export function BlogListPage() {
  const { lang } = useLanguage();
  const [params, setParams] = useSearchParams();
  const category = params.get("category");
  const { data: posts, isLoading } = useQuery({
    queryKey: ["blog", "list", lang],
    queryFn: () => fetchBlogPosts(),
  });

  const shown = useMemo(() => {
    const picked = posts ? pickPerLanguage(posts, lang) : [];
    return category ? picked.filter((p) => p.category === category) : picked;
  }, [posts, lang, category]);

  const categories = useMemo(() => {
    const set = new Set<string>();
    for (const p of posts ?? []) {
      if (p.category) set.add(p.category);
    }
    return [...set].sort();
  }, [posts]);

  if (isLoading) return <p className="text-subtle">…</p>;
  if (!posts || posts.length === 0) {
    return (
      <div className="space-y-6">
        <PageTitle>{lang === "ko" ? "블로그" : "Blog"}</PageTitle>
        <EmptyState>
          <EmptyStateIcon>
            <NotebookPen className="size-5" />
          </EmptyStateIcon>
          <EmptyStateTitle>
            {lang === "ko" ? "아직 게시물이 없습니다" : "No posts yet"}
          </EmptyStateTitle>
          <EmptyStateDescription>
            {lang === "ko"
              ? "첫 글이 곧 올라옵니다."
              : "The first post is on its way."}
          </EmptyStateDescription>
        </EmptyState>
      </div>
    );
  }

  return (
    <article className="space-y-6">
      <PageTitle>{lang === "ko" ? "블로그" : "Blog"}</PageTitle>
      {categories.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {categories.map((c) => (
            <button
              key={c}
              type="button"
              onClick={() => {
                const next = new URLSearchParams(params);
                if (c === category) next.delete("category");
                else next.set("category", c);
                setParams(next, { replace: true });
              }}
              className={`rounded-full border px-3 py-1 text-xs transition-colors ${
                c === category
                  ? "border-primary bg-primary text-primary-foreground"
                  : "border-line text-muted hover:border-border-strong"
              }`}
            >
              {c}
            </button>
          ))}
        </div>
      )}
      <ul className="space-y-3">
        {shown.map((p) => (
          <BlogPostCard
            key={p.slug}
            post={{
              slug: p.slug,
              title: p.title,
              tags: p.tags,
              published_at: p.published_at,
              category: p.category ?? null,
            }}
          />
        ))}
      </ul>
    </article>
  );
}
