import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { Film, Search, X } from "lucide-react";

import { fetchMovies, type MovieEntry, type MovieGenre, type MoviePerson } from "../../shared/api";
import { useLanguage } from "../../shared/language";
import {
  EmptyState,
  EmptyStateDescription,
  EmptyStateIcon,
  EmptyStateTitle,
} from "../../shared/ui/empty-state";
import { PageTitle } from "../../shared/ui/page-header";
import { Input } from "../../shared/ui/input";
import { cn } from "../../shared/ui/cn";
import { MovieCard } from "./MovieCard";

type MediaType = "all" | "movie" | "tv";
type SortKey = "recent" | "rating" | "year" | "title";

const selectCls =
  "h-9 rounded-md border border-line bg-canvas px-2.5 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary";

function genreLabel(
  g: MovieGenre,
  pick: (ko?: string | null, en?: string | null) => string,
) {
  return pick(g.name_ko, g.name_en) || g.name_en;
}

function personName(
  p: MoviePerson,
  pick: (ko?: string | null, en?: string | null) => string,
) {
  return pick(p.name_ko, p.name_en) || p.name_en;
}

export function MoviesPage() {
  const { pick, lang } = useLanguage();
  const { data: movies, isLoading } = useQuery({
    queryKey: ["movies", "list"],
    queryFn: fetchMovies,
  });

  const [query, setQuery] = useState("");
  const [mediaType, setMediaType] = useState<MediaType>("all");
  const [genre, setGenre] = useState<string | null>(null);
  const [year, setYear] = useState<number | null>(null);
  const [personId, setPersonId] = useState<number | null>(null);
  const [sort, setSort] = useState<SortKey>("recent");

  // 패싯: 전체 컬렉션에서 유도 (1인 사이트, 클라이언트 사이드).
  const facets = useMemo(() => {
    const genreCounts = new Map<string, { genre: MovieGenre; count: number }>();
    const yearSet = new Set<number>();
    const personCounts = new Map<number, { person: MoviePerson; count: number }>();
    for (const m of movies ?? []) {
      for (const g of m.genres) {
        const cur = genreCounts.get(g.name_en);
        if (cur) cur.count += 1;
        else genreCounts.set(g.name_en, { genre: g, count: 1 });
      }
      if (m.release_year != null) yearSet.add(m.release_year);
      for (const p of m.cast) {
        const cur = personCounts.get(p.id);
        if (cur) cur.count += 1;
        else personCounts.set(p.id, { person: p, count: 1 });
      }
    }
    return {
      genres: [...genreCounts.values()].sort((a, b) => b.count - a.count),
      years: [...yearSet].sort((a, b) => b - a),
      people: [...personCounts.values()].sort((a, b) => b.count - a.count),
    };
  }, [movies]);

  const filtered = useMemo(() => {
    if (!movies) return [];
    const q = query.trim().toLowerCase();
    let list = movies.filter((m) => {
      if (mediaType !== "all" && m.media_type !== mediaType) return false;
      if (genre != null && !m.genres.some((g) => g.name_en === genre)) return false;
      if (year != null && m.release_year !== year) return false;
      if (
        personId != null &&
        !m.cast.some((p) => p.id === personId) &&
        !m.directors.some((p) => p.id === personId)
      )
        return false;
      if (q) {
        const hay = [
          m.title,
          m.title_ko,
          m.title_en,
          ...m.cast.map((p) => `${p.name_en} ${p.name_ko ?? ""} ${p.character_name ?? ""}`),
          ...m.directors.map((p) => `${p.name_en} ${p.name_ko ?? ""}`),
          ...m.genres.map((g) => `${g.name_en} ${g.name_ko ?? ""}`),
        ]
          .join(" ")
          .toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });

    const titleOf = (m: MovieEntry) => pick(m.title_ko, m.title_en) || m.title;
    list = [...list].sort((a, b) => {
      switch (sort) {
        case "rating":
          return b.rating - a.rating;
        case "year":
          return (b.release_year ?? -1) - (a.release_year ?? -1);
        case "title":
          return titleOf(a).localeCompare(titleOf(b), lang === "ko" ? "ko" : "en");
        case "recent":
        default: {
          const ta = a.watched_at ?? a.created_at;
          const tb = b.watched_at ?? b.created_at;
          return tb.localeCompare(ta);
        }
      }
    });
    return list;
  }, [movies, query, mediaType, genre, year, personId, sort, pick, lang]);

  const hasFilters = !!query || mediaType !== "all" || genre || year != null || personId != null;
  // 매체 토글은 두 매체가 모두 존재할 때만 의미가 있다 — 단일 타입 컬렉션에서는
  // "드라마 0" 같은 죽은 필터를 만들지 않는다 (facets와 같은 데이터 기반 규칙).
  const hasBothMediaTypes =
    !!movies?.some((m) => m.media_type === "tv") && !!movies?.some((m) => m.media_type === "movie");

  const clearAll = () => {
    setMediaType("all");
    setGenre(null);
    setYear(null);
    setPersonId(null);
  };

  const activePerson = personId != null ? facets.people.find((p) => p.person.id === personId) : null;

  if (isLoading) return <p className="text-subtle">…</p>;

  const titleText = lang === "ko" ? "영화·드라마" : "Movies & TV";

  if (!movies || movies.length === 0) {
    return (
      <div className="space-y-6">
        <PageTitle>{titleText}</PageTitle>
        <EmptyState>
          <EmptyStateIcon>
            <Film />
          </EmptyStateIcon>
          <EmptyStateTitle>
            {lang === "ko" ? "기록된 작품이 없습니다" : "No movies yet"}
          </EmptyStateTitle>
          <EmptyStateDescription>
            {lang === "ko"
              ? "`oxibuilder movies add` 로 작품을 추가하세요."
              : "Add a title with `oxibuilder movies add`."}
          </EmptyStateDescription>
        </EmptyState>
      </div>
    );
  }

  return (
    <article className="space-y-6">
      <header className="space-y-1.5">
        <PageTitle>{titleText}</PageTitle>
        <p className="text-sm text-muted">
          {lang === "ko" ? `${movies.length}편의 기록` : `${movies.length} records`}
        </p>
        <Link
          to="/movies/stats"
          className="inline-flex items-center gap-0.5 text-sm text-primary hover:underline"
        >
          {lang === "ko" ? "통계 보기" : "Statistics"}
          <span aria-hidden>›</span>
        </Link>
      </header>

      {/* 검색 + 정렬 */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-subtle" />
          <Input
            className="pl-9"
            placeholder={lang === "ko" ? "제목, 배우, 감독, 장르 검색…" : "Search title, cast, director, genre…"}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <select
          className={selectCls}
          value={sort}
          onChange={(e) => setSort(e.target.value as SortKey)}
          aria-label={lang === "ko" ? "정렬" : "Sort"}
        >
          <option value="recent">{lang === "ko" ? "최근 감상순" : "Recently watched"}</option>
          <option value="rating">{lang === "ko" ? "평점순" : "Rating"}</option>
          <option value="year">{lang === "ko" ? "개봉연도순" : "Year"}</option>
          <option value="title">{lang === "ko" ? "제목순" : "Title"}</option>
        </select>
      </div>

      {/* 매체 + 연도 + 배우 */}
      <div className="flex flex-wrap items-center gap-2">
        {hasBothMediaTypes && (
          <div className="inline-flex rounded-md border border-line p-0.5">
            {(["all", "movie", "tv"] as MediaType[]).map((t) => (
              <button
                key={t}
                type="button"
                onClick={() => setMediaType(t)}
                className={cn(
                  "rounded px-2.5 py-1 text-xs font-medium transition-colors",
                  mediaType === t
                    ? "bg-primary text-primary-foreground"
                    : "text-subtle hover:text-foreground",
                )}
              >
                {t === "all"
                  ? lang === "ko"
                    ? "전체"
                    : "All"
                  : t === "movie"
                    ? lang === "ko"
                      ? "영화"
                      : "Movies"
                    : lang === "ko"
                      ? "드라마"
                      : "TV"}
              </button>
            ))}
          </div>
        )}

        <select
          className={selectCls}
          value={year ?? ""}
          onChange={(e) => setYear(e.target.value ? Number(e.target.value) : null)}
          aria-label={lang === "ko" ? "연도" : "Year"}
        >
          <option value="">{lang === "ko" ? "모든 연도" : "All years"}</option>
          {facets.years.map((y) => (
            <option key={y} value={y}>
              {y}
            </option>
          ))}
        </select>

        <select
          className={cn(selectCls, "max-w-[14rem]")}
          value={personId ?? ""}
          onChange={(e) => setPersonId(e.target.value ? Number(e.target.value) : null)}
          aria-label={lang === "ko" ? "출연진" : "Cast"}
        >
          <option value="">{lang === "ko" ? "모든 출연진" : "All cast"}</option>
          {facets.people.map(({ person, count }) => (
            <option key={person.id} value={person.id}>
              {`${personName(person, pick)} (${count})`}
            </option>
          ))}
        </select>
      </div>

      {/* 장르 칩 */}
      {facets.genres.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          <button
            type="button"
            onClick={() => setGenre(null)}
            className={cn(
              "rounded-md border px-3 py-1.5 text-xs font-medium transition-colors",
              genre === null
                ? "border-primary bg-primary text-primary-foreground"
                : "border-line text-muted hover:border-line-strong hover:text-foreground",
            )}
          >
            {lang === "ko" ? "전체" : "All"}{" "}
            <span className="tabular-nums opacity-60">{movies.length}</span>
          </button>
          {facets.genres.map(({ genre: g, count }) => {
            const active = genre === g.name_en;
            return (
              <button
                key={g.name_en}
                type="button"
                onClick={() => setGenre(active ? null : g.name_en)}
                className={cn(
                  "rounded-md border px-3 py-1.5 text-xs font-medium transition-colors",
                  active
                    ? "border-primary bg-primary text-primary-foreground"
                    : "border-line text-muted hover:border-line-strong hover:text-foreground",
                )}
              >
                {genreLabel(g, pick)}{" "}
                <span className="tabular-nums opacity-60">{count}</span>
              </button>
            );
          })}
        </div>
      )}

      {/* 활성 필터 */}
      {hasFilters && (
        <div className="flex flex-wrap items-center gap-1.5 text-xs">
          <span className="text-subtle">{lang === "ko" ? "필터:" : "Filters:"}</span>
          {mediaType !== "all" && (
            <FilterChip label={mediaType === "movie" ? (lang === "ko" ? "영화" : "Movies") : (lang === "ko" ? "드라마" : "TV")} onClear={() => setMediaType("all")} />
          )}
          {genre && (
            <FilterChip
              label={genreLabel(facets.genres.find((x) => x.genre.name_en === genre)!.genre, pick)}
              onClear={() => setGenre(null)}
            />
          )}
          {year != null && <FilterChip label={String(year)} onClear={() => setYear(null)} />}
          {activePerson && (
            <FilterChip label={personName(activePerson.person, pick)} onClear={() => setPersonId(null)} />
          )}
          {query && <FilterChip label={`"${query}"`} onClear={() => setQuery("")} />}
          <button
            type="button"
            onClick={clearAll}
            className="text-subtle underline-offset-2 hover:text-foreground hover:underline"
          >
            {lang === "ko" ? "전체 해제" : "Clear all"}
          </button>
        </div>
      )}

      {filtered.length === 0 ? (
        <EmptyState>
          <EmptyStateIcon>
            <Search />
          </EmptyStateIcon>
          <EmptyStateTitle>
            {lang === "ko" ? "조건에 맞는 작품이 없습니다" : "No matches"}
          </EmptyStateTitle>
          <EmptyStateDescription>
            {lang === "ko" ? "필터를 조정해 보세요." : "Try adjusting your filters."}
          </EmptyStateDescription>
        </EmptyState>
      ) : (
        <ul className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
          {filtered.map((m, i) => (
            <li key={m.id}>
              <MovieCard
                index={i}
                movie={{
                  id: m.id,
                  slug: m.slug,
                  title: m.title,
                  title_ko: m.title_ko,
                  title_en: m.title_en,
                  media_type: m.media_type === "tv" ? "tv" : "movie",
                  poster_path: m.poster_path,
                  release_year: m.release_year,
                  genres: m.genres,
                  directors: m.directors,
                }}
                pick={pick}
                lang={lang}
                activeGenre={genre}
                activePerson={personId}
                onPickGenre={(key) => setGenre((prev) => (prev === key ? null : key))}
                onPickPerson={(id) => setPersonId((prev) => (prev === id ? null : id))}
              />
            </li>
          ))}
        </ul>
      )}
    </article>
  );
}

function FilterChip({ label, onClear }: { label: string; onClear: () => void }) {
  return (
    <span className="inline-flex items-center gap-1 rounded-full border border-line bg-surface px-2 py-0.5 text-foreground">
      {label}
      <button
        type="button"
        onClick={onClear}
        className="text-subtle hover:text-foreground"
        aria-label="clear"
      >
        <X className="size-3" />
      </button>
    </span>
  );
}
