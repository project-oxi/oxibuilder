import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router";
import { ArrowLeft, Film } from "lucide-react";

import { fetchMovies } from "../../shared/api";
import { useLanguage } from "../../shared/language";
import { PageTitle } from "../../shared/ui/page-header";
import { RatingStars } from "../../shared/RatingStars";
import { pickVariant, useOptimizedImage } from "../../shared/useOptimizedImage";
import {
  EmptyState,
  EmptyStateDescription,
  EmptyStateIcon,
  EmptyStateTitle,
} from "../../shared/ui/empty-state";

function posterUrl(path: string | null, width: number) {
  // Multi-segment absolute paths are local mounted refs ("/posters/x.jpg"),
  // not raw TMDB paths — pass through without the image.tmdb.org prefix.
  if (path && path.startsWith("/") && path.slice(1).includes("/")) return path;
  return path ? `https://image.tmdb.org/t/p/w${width}${path}` : null;
}

function fmtRuntime(min: number | null) {
  if (!min || min <= 0) return null;
  const h = Math.floor(min / 60);
  const m = min % 60;
  return h > 0 ? (m > 0 ? `${h}h ${m}m` : `${h}h`) : `${m}m`;
}

export function MovieDetailPage() {
  const { slug } = useParams<{ slug: string }>();
  const { pick, lang } = useLanguage();
  const ko = lang === "ko";
  const { data: movies, isLoading } = useQuery({ queryKey: ["movies"], queryFn: fetchMovies });

  if (isLoading) return <p className="text-subtle">…</p>;
  const movie = movies?.find((m) => m.slug === slug);

  if (!movie) {
    return (
      <article className="space-y-6">
        <Link to="/movies" className="inline-flex items-center gap-2 text-subtle transition-colors hover:text-foreground">
          <ArrowLeft className="size-4" /> {ko ? "영화 목록" : "Movies"}
        </Link>
        <EmptyState>
          <EmptyStateIcon>
            <Film />
          </EmptyStateIcon>
          <EmptyStateTitle>{ko ? "없는 영화" : "Not found"}</EmptyStateTitle>
          <EmptyStateDescription>
            {ko ? "해당 영화를 찾을 수 없습니다." : "That movie could not be found."}
          </EmptyStateDescription>
        </EmptyState>
      </article>
    );
  }

  const title = pick(movie.title_ko, movie.title_en) || movie.title;
  const poster = posterUrl(movie.poster_path, 500);
  const optimized = useOptimizedImage(poster);
  const variant = optimized ? pickVariant(optimized) : null;
  const runtime = fmtRuntime(movie.runtime_min);
  const synopsis = pick(movie.review_ko, movie.review_en);

  return (
    <article className="space-y-8">
      <Link to="/movies" className="inline-flex items-center gap-2 text-subtle transition-colors hover:text-foreground">
        <ArrowLeft className="size-4" /> {ko ? "영화 목록" : "Movies"}
      </Link>

      <div className="flex flex-col gap-6 sm:flex-row">
        {variant ? (
          <img
            src={variant.url}
            srcSet={optimized!.srcset.map((s) => `${s.url} ${s.w}w`).join(", ")}
            sizes="160px"
            width={optimized!.width}
            height={optimized!.height}
            alt={title}
            className="w-40 shrink-0 rounded-lg border border-line object-cover shadow-sm"
            loading="lazy"
            decoding="async"
          />
        ) : poster ? (
          <img
            src={poster}
            alt={title}
            className="w-40 shrink-0 rounded-lg border border-line object-cover shadow-sm"
            loading="lazy"
          />
        ) : (
          <div className="flex w-40 shrink-0 items-center justify-center rounded-lg border border-line bg-surface text-subtle">
            <Film className="size-8" />
          </div>
        )}

        <div className="space-y-3">
          <PageTitle>{title}</PageTitle>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-subtle">
            {movie.release_year != null && <span>{movie.release_year}</span>}
            <span>{movie.media_type === "tv" ? (ko ? "드라마" : "TV") : ko ? "영화" : "Film"}</span>
            {runtime && <span>{runtime}</span>}
          </div>
          <RatingStars value={movie.rating} />
          {movie.genres.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {movie.genres.map((g) => (
                <span key={g.name_en} className="rounded-full border border-line bg-surface px-2.5 py-0.5 text-xs text-muted">
                  {pick(g.name_ko, g.name_en) || g.name_en}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>


      {synopsis && <p className="max-w-prose leading-relaxed text-foreground">{synopsis}</p>}

      {movie.directors.length > 0 && (
        <section className="space-y-1">
          <h2 className="font-serif text-lg font-semibold text-foreground">{ko ? "감독" : "Directors"}</h2>
          <p className="text-muted">
            {movie.directors.map((d) => pick(d.name_ko, d.name_en) || d.name_en).join(", ")}
          </p>
        </section>
      )}

      {movie.cast.length > 0 && (
        <section className="space-y-1">
          <h2 className="font-serif text-lg font-semibold text-foreground">{ko ? "출연진" : "Cast"}</h2>
          <ul className="space-y-0.5 text-muted">
            {movie.cast.map((p) => (
              <li key={p.id}>
                {pick(p.name_ko, p.name_en) || p.name_en}
                {p.character_name && <span className="text-subtle"> · {p.character_name}</span>}
              </li>
            ))}
          </ul>
        </section>
      )}
    </article>
  );
}
