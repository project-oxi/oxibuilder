import { Film } from "lucide-react";
import { Link } from "react-router";

import type { MovieGenre, MoviePerson } from "../../shared/api";
import { pickVariant, useOptimizedImage } from "../../shared/useOptimizedImage";
import { cn } from "../../shared/ui/cn";

export interface MovieCardData {
  id: number;
  slug: string;
  title: string;
  title_ko: string | null;
  title_en: string | null;
  media_type: "movie" | "tv";
  poster_path: string | null;
  release_year: number | null;
  genres: MovieGenre[];
  directors: MoviePerson[];
}

type Pick = (ko?: string | null, en?: string | null) => string;

const EAGER_COUNT = 10;

function posterUrl(path: string | null) {
  // Multi-segment absolute paths are local mounted refs ("/posters/x.jpg"),
  // not raw TMDB paths — pass through without the image.tmdb.org prefix.
  if (path && path.startsWith("/") && path.slice(1).includes("/")) return path;
  return path ? `https://image.tmdb.org/t/p/w342${path}` : null;
}

/**
 * Poster-first grid card in the legacy-collection language: full 2:3 poster,
 * hairline footer with title / year·director / genre tags. The whole card
 * links to the detail page; genre tags stay clickable above the link overlay.
 */
export function MovieCard({
  movie,
  index,
  pick,
  lang,
  activeGenre,
  activePerson,
  onPickGenre,
  onPickPerson,
}: {
  movie: MovieCardData;
  index?: number;
  pick: Pick;
  lang: "ko" | "en";
  activeGenre?: string | null;
  activePerson?: number | null;
  onPickGenre?: (key: string) => void;
  onPickPerson?: (id: number) => void;
}) {
  const img = posterUrl(movie.poster_path);
  const optimized = useOptimizedImage(img);
  const variant = optimized ? pickVariant(optimized) : null;
  const eager = index != null && index < EAGER_COUNT;
  const title = pick(movie.title_ko, movie.title_en) || movie.title;
  const director = movie.directors[0];
  const directorName = director
    ? pick(director.name_ko, director.name_en) || director.name_en
    : null;
  const typeLabel =
    movie.media_type === "tv"
      ? lang === "ko"
        ? "드라마"
        : "TV"
      : lang === "ko"
        ? "영화"
        : "Film";
  const meta = [movie.release_year?.toString(), directorName ?? typeLabel]
    .filter(Boolean)
    .join(" · ");

  const imgCls =
    "h-full w-full object-cover transition-transform duration-300 group-hover:scale-105 motion-reduce:transform-none";

  return (
    <article className="group relative overflow-hidden rounded-lg border border-line transition-colors duration-200 hover:border-line-strong">
      <div className="aspect-[2/3] overflow-hidden bg-surface">
        {variant ? (
          <img
            src={variant.url}
            srcSet={optimized!.srcset.map((s) => `${s.url} ${s.w}w`).join(", ")}
            sizes="(min-width: 1024px) 18vw, (min-width: 640px) 24vw, 46vw"
            width={optimized!.width}
            height={optimized!.height}
            alt=""
            loading={eager ? "eager" : "lazy"}
            fetchPriority={eager ? "high" : "auto"}
            decoding="async"
            className={imgCls}
          />
        ) : img ? (
          <img
            src={img}
            alt=""
            loading={eager ? "eager" : "lazy"}
            fetchPriority={eager ? "high" : "auto"}
            className={imgCls}
          />
        ) : (
          <div className="flex h-full items-center justify-center text-subtle">
            <Film className="size-6" />
          </div>
        )}
      </div>

      {/* Whole-card link; footer content stacks above it. */}
      <Link to={`/movies/${movie.slug}`} className="absolute inset-0" aria-label={title}>
        <span className="sr-only">{title}</span>
      </Link>

      <div className="relative space-y-1 border-t border-line p-3">
        <h3 className="truncate text-sm font-medium text-foreground">{title}</h3>
        {meta && <p className="truncate text-xs tabular-nums text-subtle">{meta}</p>}
        {movie.genres.length > 0 && (
          <div className="flex flex-wrap gap-1 pt-0.5">
            {movie.genres.slice(0, 3).map((g) => {
              const key = g.name_en;
              const label = pick(g.name_ko, g.name_en) || g.name_en;
              const active = activeGenre === key;
              const tag = cn(
                "rounded-xs border px-1.5 py-0.5 text-[10px] transition-colors",
                active
                  ? "border-primary bg-primary text-primary-foreground"
                  : "border-line text-subtle",
              );
              return onPickGenre ? (
                <button key={key} type="button" onClick={() => onPickGenre(key)} className={tag}>
                  {label}
                </button>
              ) : (
                <span key={key} className={tag}>
                  {label}
                </span>
              );
            })}
          </div>
        )}
      </div>
    </article>
  );
}
