import { useState, type FormEvent } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { SearchIcon } from "lucide-react";

import { searchAll } from "../shared/api";
import { useLanguage } from "../shared/language";
import { Button } from "../shared/ui/button";
import { Input } from "../shared/ui/input";
import { PageTitle } from "../shared/ui/page-header";

export function SearchPage() {
  const { lang } = useLanguage();
  const [q, setQ] = useState("");
  const [submitted, setSubmitted] = useState("");

  const { data: hits, isFetching } = useQuery({
    queryKey: ["search", submitted],
    queryFn: () => searchAll(submitted, lang),
    enabled: submitted.length > 0,
  });

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    setSubmitted(q.trim());
  }

  // doc_id → 프론트 경로 추정. novels "slug/chapters/N" → /novels/{slug}, 그 외 /{ext}/{doc_id}.
  function docUrl(extId: string, docId: string): string {
    if (docId.includes("/")) {
      return `/${extId}/${docId.split("/")[0]}`;
    }
    return `/${extId}/${docId}`;
  }

  return (
    <article className="space-y-6">
      <PageTitle>{lang === "ko" ? "검색" : "Search"}</PageTitle>

      <form role="search" onSubmit={onSubmit} className="flex gap-2">
        <Input
          type="search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder={lang === "ko" ? "검색어 입력…" : "Search…"}
          aria-label={lang === "ko" ? "검색어" : "Search query"}
          autoFocus
        />
        <Button type="submit">
          <SearchIcon />
          <span className="hidden sm:inline">
            {lang === "ko" ? "검색" : "Search"}
          </span>
        </Button>
      </form>

      {isFetching && <p className="text-subtle">…</p>}

      {submitted && !isFetching && hits && hits.length === 0 && (
        <p className="text-subtle">
          {lang === "ko"
            ? `"${submitted}"에 대한 결과가 없습니다.`
            : `No results for "${submitted}".`}
        </p>
      )}

      {hits && hits.length > 0 && (
        <ul className="divide-y divide-line border-y border-line">
          {hits.map((h, i) => (
            <li key={`${h.extension_id}-${h.doc_id}-${i}`} className="group">
              <Link
                to={docUrl(h.extension_id, h.doc_id)}
                className="block py-5 text-foreground no-underline"
              >
                <p className="text-[10px] font-medium uppercase tracking-widest text-subtle">
                  {h.extension_id}
                </p>
                <h2 className="mt-1 font-serif text-lg font-semibold tracking-tight transition-colors duration-200 group-hover:text-primary">
                  {h.title}
                </h2>
                <p
                  className="mt-1 text-sm text-muted"
                  dangerouslySetInnerHTML={{ __html: h.snippet }}
                />
              </Link>
            </li>
          ))}
        </ul>
      )}
    </article>
  );
}
