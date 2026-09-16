import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { ArrowLeft } from "lucide-react";

import { fetchMovies } from "../../shared/api";
import { useLanguage } from "../../shared/language";
import { PageTitle } from "../../shared/ui/page-header";
import { BarRow, ColumnChart, SummaryBand } from "../../shared/stats/StatsKit";
import { computeMovieStats } from "../../shared/stats/computeMovieStats";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="space-y-2">
      <h2 className="font-serif text-lg font-semibold text-foreground">{title}</h2>
      {children}
    </section>
  );
}

export function MoviesStatsPage() {
  const { lang } = useLanguage();
  const ko = lang === "ko";
  const { data: movies, isLoading } = useQuery({ queryKey: ["movies"], queryFn: fetchMovies });

  const stats = useMemo(() => (movies ? computeMovieStats(movies) : null), [movies]);

  if (isLoading) return <p className="text-subtle">…</p>;
  if (!movies || movies.length === 0 || !stats) {
    return (
      <article className="space-y-4">
        <PageTitle>{ko ? "영화 통계" : "Movie Stats"}</PageTitle>
        <p className="text-subtle">{ko ? "아직 영화가 없습니다." : "No movies yet."}</p>
      </article>
    );
  }

  const yearRange =
    stats.yearMax > 0 ? `${stats.yearMin}–${stats.yearMax}` : "—";

  return (
    <article className="space-y-10">
      <div className="flex items-center gap-3">
        <Link
          to="/movies"
          className="text-subtle transition-colors hover:text-foreground"
          aria-label={ko ? "목록으로" : "Back to list"}
        >
          <ArrowLeft className="size-5" />
        </Link>
        <PageTitle>{ko ? "영화 통계" : "Movie Stats"}</PageTitle>
      </div>

      <SummaryBand
        items={[
          { label: ko ? "편수" : "Titles", value: stats.total },
          { label: ko ? "감독" : "Directors", value: stats.directorCount },
          { label: ko ? "배우" : "Actors", value: stats.actorCount },
          { label: ko ? "국가" : "Countries", value: stats.nationCount },
          { label: ko ? "연도" : "Years", value: yearRange },
          { label: ko ? "평균 러닝타임" : "Avg runtime", value: stats.avgRuntime ? `${Math.round(stats.avgRuntime)}${ko ? "분" : "m"}` : null },
          { label: ko ? "평균 평점" : "Avg rating", value: stats.ratingMean ? stats.ratingMean.toFixed(1) : null },
        ]}
      />

      {stats.years.length > 0 && (
        <Section title={ko ? "연도별" : "By year"}>
          <ColumnChart data={stats.years} max={Math.max(...stats.years.map((y) => y.count), 1)} />
        </Section>
      )}

      {stats.nations.length > 0 && (
        <Section title={ko ? "국가" : "Countries"}>
          {stats.nations.map((n) => (
            <BarRow key={n.name} name={n.name} count={n.count} max={stats.nations[0].count} />
          ))}
        </Section>
      )}

      {stats.genres.length > 0 && (
        <Section title={ko ? "장르" : "Genres"}>
          {stats.genres.map((g) => (
            <BarRow key={g.name} name={g.name} count={g.count} max={stats.genres[0].count} />
          ))}
        </Section>
      )}

      {stats.actors.length > 0 && (
        <Section title={ko ? "출연진" : "Cast"}>
          {stats.actors.map((a) => (
            <BarRow key={a.name} name={a.name} count={a.count} max={stats.actors[0].count} />
          ))}
        </Section>
      )}

      {stats.directors.length > 0 && (
        <Section title={ko ? "감독" : "Directors"}>
          {stats.directors.map((d) => (
            <BarRow key={d.name} name={d.name} count={d.count} max={stats.directors[0].count} />
          ))}
        </Section>
      )}

      {stats.runtimeBuckets.length > 0 && (
        <Section title={ko ? "러닝타임" : "Runtime"}>
          {stats.runtimeBuckets.map((b) => (
            <BarRow key={b.name} name={b.name} count={b.count} max={Math.max(...stats.runtimeBuckets.map((x) => x.count), 1)} />
          ))}
        </Section>
      )}

      {stats.ratingBuckets.length > 0 && (
        <Section title={ko ? "평점" : "Rating"}>
          {stats.ratingBuckets.map((b) => (
            <BarRow key={b.name} name={b.name} count={b.count} max={Math.max(...stats.ratingBuckets.map((x) => x.count), 1)} />
          ))}
        </Section>
      )}
    </article>
  );
}
