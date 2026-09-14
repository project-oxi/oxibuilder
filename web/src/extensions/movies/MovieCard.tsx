import { Film, RotateCcw } from "lucide-react";

import type { MovieGenre, MoviePerson } from "../../shared/api";
import { Card } from "../../shared/ui/card";
import { RatingStars } from "../../shared/RatingStars";
import { cn } from "../../shared/ui/cn";
import { pickVariant, useOptimizedImage } from "../../shared/useOptimizedImage";

export interface MovieCardData {
  id: number;
  slug: string;
  title: string;
  title_ko: string | null;
  title_en: string | null;
  media_type: "movie" | "tv";
  poster_path: string | null;
  release_year: number | null;
  runtime_min: number | null;
  rating: number;
  review_ko: string | null;
  review_en: string | null;
  rewatch: boolean;
  genres: MovieGenre[];
  cast: MoviePerson[];
}

type Pick = (ko?: string | null, en?: string | null) => string;

const EAGER_COUNT = 10;

function posterUrl(path: string | null) {
  if (!path) return null;
  // Local refs (mounted static dirs, e.g. "/posters/x.jpg") pass through as-is;
  // only raw TMDB paths (single segment) get the `w500` prefix, which matches
  // the Rust build's canonical width for the manifest key
  // (`crates/oxibuilder-ext-movies/src/lib.rs::external_image_urls`); the SPA
  // must look up the same URL the build emitted. `srcset`/`sizes` below
  // handle responsive sizing for the actual rendered display.
  if (path.startsWith("/") && path.slice(1).includes("/")) return path;
  return `https://image.tmdb.org/t/p/w500${path}`;
}

function fmtRuntime(min: number | null): string | null {
  if (!min || min <= 0) return null;
  const h = Math.floor(min / 60);
  const m = min % 60;
  return h > 0 ? (m > 0 ? `${h}h ${m}m` : `${h}h`) : `${m}m`;
}

interface MovieCardProps {
  movie: MovieCardData;
  index?: number;
  pick: Pick;
  lang: "ko" | "en";
  activeGenre?: string | null;
  activePerson?: number | null;
  onPickGenre?: (key: string) => void;
  onPickPerson?: (id: number) => void;
}

export function MovieCard({
  movie,
  index,
  pick,
  lang,
  activeGenre,
  activePerson,
  onPickGenre,
  onPickPerson,
}: MovieCardProps) {
  const img = posterUrl(movie.poster_path);
  const optimized = useOptimizedImage(img);
  const variant = optimized ? pickVariant(optimized) : null;
  const eager = index != null && index < EAGER_COUNT;
  const title = pick(movie.title_ko, movie.title_en) || movie.title;
  const runtime = fmtRuntime(movie.runtime_min);
  const review = pick(movie.review_ko, movie.review_en);
  const leads = movie.cast.slice(0, 3);

  return (
    <Card className="flex h-full gap-4 p-4">
      {variant ? (
        <img
          src={variant.url}
          srcSet={optimized!.srcset.map((s) => `${s.url} ${s.w}w`).join(", ")}
          sizes="80px"
          width={optimized!.width}
          height={optimized!.height}
          alt=""
          loading={eager ? "eager" : "lazy"}
          fetchPriority={eager ? "high" : "auto"}
          decoding="async"
          className="w-16 shrink-0 rounded-md object-cover sm:w-20"
        />
      ) : img ? (
        <img
          src={img}
          alt=""
          loading={eager ? "eager" : "lazy"}
          fetchPriority={eager ? "high" : "auto"}
          className="w-16 shrink-0 rounded-md object-cover sm:w-20"
        />
      ) : (
        <div className="flex w-16 shrink-0 items-center justify-center rounded-md bg-surface text-subtle sm:w-20">
          <Film className="size-5" />
        </div>
      )}
      <div className="min-w-0 flex-1 space-y-1.5">
        <div className="flex items-start justify-between gap-2">
          <h2 className="font-serif text-base font-semibold leading-tight text-foreground">
            {title}
          </h2>
          {movie.rewatch && (
            <span
              title={lang === "ko" ? "다시 본 작품" : "Rewatched"}
              className="mt-0.5 shrink-0 text-subtle"
            >
              <RotateCcw className="size-3.5" />
            </span>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-subtle">
          <span className="uppercase tracking-wide">
            {movie.media_type === "tv"
              ? lang === "ko"
                ? "드라마"
                : "TV"
              : lang === "ko"
                ? "영화"
                : "Movie"}
          </span>
          {movie.release_year != null && <span>· {movie.release_year}</span>}
          {runtime && <span>· {runtime}</span>}
        </div>

        <RatingStars value={movie.rating} size="sm" />

        {movie.genres.length > 0 && (
          <div className="flex flex-wrap gap-1 pt-0.5">
            {movie.genres.slice(0, 4).map((g) => {
              const key = g.name_en;
              const label = pick(g.name_ko, g.name_en);
              const active = activeGenre === key;
              return onPickGenre ? (
                <button
                  key={key}
                  type="button"
                  onClick={() => onPickGenre(key)}
                  className={cn(
                    "rounded-full border px-2 py-0.5 text-[11px] transition-colors",
                    active
                      ? "border-primary bg-primary/10 text-primary"
                      : "border-line text-subtle hover:border-primary/40 hover:text-foreground",
                  )}
                >
                  {label}
                </button>
              ) : (
                <span
                  key={key}
                  className="rounded-full border border-line px-2 py-0.5 text-[11px] text-subtle"
                >
                  {label}
                </span>
              );
            })}
          </div>
        )}

        {leads.length > 0 && (
          <p className="text-xs text-subtle">
            <span className="text-muted">
              {lang === "ko" ? "출연" : "Cast"}:{" "}
            </span>
            {leads.map((p, i) => {
              const name = pick(p.name_ko, p.name_en) || p.name_en;
              const active = activePerson === p.id;
              return (
                <span key={p.id}>
                  {onPickPerson ? (
                    <button
                      type="button"
                      onClick={() => onPickPerson(p.id)}
                      className={cn(
                        "underline-offset-2 hover:underline",
                        active ? "text-primary underline" : "hover:text-foreground",
                      )}
                    >
                      {name}
                    </button>
                  ) : (
                    name
                  )}
                  {p.character_name ? (
                    <span className="text-muted"> ({p.character_name})</span>
                  ) : null}
                  {i < leads.length - 1 ? ", " : ""}
                </span>
              );
            })}
          </p>
        )}

        {review && (
          <p className="line-clamp-2 pt-0.5 text-sm text-subtle">{review}</p>
        )}
      </div>
    </Card>
  );
}
