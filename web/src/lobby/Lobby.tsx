import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { useEffect, useState, type ComponentType } from "react";
import {
  Activity,
  BookOpen,
  Bookmark,
  Film,
  FolderGit2,
  Hash,
  type LucideProps,
  PenLine,
  Sparkles,
  UserRound,
} from "lucide-react";

import { fetchManifest, type ManifestExtension, type ManifestMount } from "../shared/api";
import { useLanguage } from "../shared/language";
import type { Lang } from "../shared/language";
import { cn } from "../shared/ui/cn";

type DisplayMode = "canvas" | "grid" | "list";

/** Known extension → icon map (consumed by the editorial hub). Unknown extensions fall back to Sparkles. */
export const EXT_ICONS: Record<string, ComponentType<LucideProps>> = {
  blog: PenLine,
  books: BookOpen,
  movies: Film,
  projects: FolderGit2,
  links: Hash,
  scraps: Bookmark,
  activity: Activity,
  profile: UserRound,
};

function prefersReducedMotion(): boolean {
  if (typeof window === "undefined" || !window.matchMedia) return false;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function displayName(ext: ManifestExtension, lang: Lang): string {
  return (lang === "ko" ? ext.display_name.ko : ext.display_name.en) ?? ext.id;
}

/** First localized line across ko/en, falling over to the other language. */
export function localizedLine(
  ko: string | null | undefined,
  en: string | null | undefined,
  lang: Lang,
): string | null {
  return (lang === "ko" ? ko : en) ?? (lang === "ko" ? en : ko) ?? null;
}

export function Lobby() {
  const { data: manifest } = useQuery({ queryKey: ["manifest"], queryFn: fetchManifest });
  const { lang } = useLanguage();
  const [reduced, setReduced] = useState(prefersReducedMotion);

  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const handler = () => setReduced(mq.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  if (!manifest) {
    return (
      <>
        <h1 className="sr-only">{lang === "ko" ? "로딩 중" : "Loading"}</h1>
        <div className="mx-auto grid w-full max-w-2xl grid-cols-1 gap-4 sm:grid-cols-2">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-32 animate-pulse rounded-lg bg-surface" />
          ))}
        </div>
      </>
    );
  }

  // Legacy-baseline lobby: one centered identity block, then a narrow index of
  // generous typographic cards (title + one muted line — no icons, no paths).
  const hero = (
    <header className="mb-16 text-center">
      <h1 className="font-serif text-5xl font-bold tracking-tight text-foreground sm:text-6xl">
        {manifest.site.name}
      </h1>
      {manifest.site.tagline && (
        <p className="mt-3 text-sm text-muted">{manifest.site.tagline}</p>
      )}
    </header>
  );

  const exts = [...manifest.extensions].sort(
    (a, b) => a.lobby.display_order - b.lobby.display_order,
  );

  // doc/03 §3.6 — 접근성: reduced-motion 시 canvas → grid 강제 폴백.
  const rawMode = exts[0]?.lobby.display_mode ?? "grid";
  const mode: DisplayMode = reduced && rawMode === "canvas" ? "grid" : rawMode;
  if (mode === "list") {
    return (
      <>
        {hero}
        <div className="mx-auto max-w-2xl divide-y divide-line rounded-lg border border-line">
          {exts.map((ext) => (
            <LobbyRow key={ext.id} ext={ext} lang={lang} />
          ))}
          {manifest.mounts.map((m) => (
            <MountRow key={m.id} mount={m} lang={lang} />
          ))}
        </div>
      </>
    );
  }

  const floating = mode === "canvas";
  return (
    <>
      {hero}
      <nav
        className="mx-auto grid w-full max-w-2xl grid-cols-1 gap-4 sm:grid-cols-2"
        aria-label={lang === "ko" ? "주요 섹션" : "Sections"}
      >
        {exts.map((ext, i) => (
          <LobbyCard
            key={ext.id}
            ext={ext}
            lang={lang}
            floating={floating}
            delay={i % 3}
          />
        ))}
        {manifest.mounts.map((m) => (
          <MountCard key={m.id} mount={m} lang={lang} />
        ))}
      </nav>
    </>
  );
}

function LobbyCard({
  ext,
  lang,
  floating,
  delay,
}: {
  ext: ManifestExtension;
  lang: Lang;
  floating: boolean;
  delay: number;
}) {
  const name = displayName(ext, lang);
  const desc = ext.description
    ? localizedLine(ext.description.ko, ext.description.en, lang)
    : null;

  return (
    <Link
      to={`/${ext.id}`}
      data-extension={ext.id}
      style={floating && delay ? { animationDelay: `${-delay * 2}s` } : undefined}
      className={cn(
        "group rounded-lg border border-line p-8 transition-colors duration-200 ease-out",
        "hover:border-line-strong hover:bg-surface",
        "motion-reduce:transition-none",
        floating && "animate-[lobby-float_6s_ease-in-out_infinite] motion-reduce:animate-none",
      )}
    >
      <h2 className="font-serif text-2xl font-semibold tracking-tight text-foreground">
        {name}
      </h2>
      {desc && <p className="mt-2 text-sm text-muted">{desc}</p>}
    </Link>
  );
}

function LobbyRow({
  ext,
  lang,
}: {
  ext: ManifestExtension;
  lang: Lang;
}) {
  const name = displayName(ext, lang);
  const desc = ext.description
    ? localizedLine(ext.description.ko, ext.description.en, lang)
    : null;

  return (
    <Link
      to={`/${ext.id}`}
      data-extension={ext.id}
      className="group block px-6 py-5 transition-colors hover:bg-surface"
    >
      <div className="flex items-baseline justify-between gap-4">
        <span className="font-serif text-lg font-semibold tracking-tight text-foreground">
          {name}
        </span>
        <span
          aria-hidden
          className="text-subtle opacity-0 transition-opacity group-hover:opacity-100"
        >
          →
        </span>
      </div>
      {desc && <p className="mt-1 text-sm text-muted">{desc}</p>}
    </Link>
  );
}

export function mountName(m: ManifestMount, lang: Lang): string {
  return (lang === "ko" ? m.display_name.ko : m.display_name.en) ?? m.id;
}

function MountCard({ mount, lang }: { mount: ManifestMount; lang: Lang }) {
  return (
    <a
      href={`${mount.path}/`}
      data-mount={mount.id}
      {...(mount.open_in_new_tab ? { target: "_blank", rel: "noopener" } : {})}
      className="group rounded-lg border border-line p-8 transition-colors duration-200 ease-out hover:border-line-strong hover:bg-surface"
    >
      <h2 className="font-serif text-2xl font-semibold tracking-tight text-foreground">
        {mountName(mount, lang)}
      </h2>
      {mount.description && <p className="mt-2 text-sm text-muted">{mount.description}</p>}
    </a>
  );
}

function MountRow({ mount, lang }: { mount: ManifestMount; lang: Lang }) {
  return (
    <a
      href={`${mount.path}/`}
      data-mount={mount.id}
      {...(mount.open_in_new_tab ? { target: "_blank", rel: "noopener" } : {})}
      className="group block px-6 py-5 transition-colors hover:bg-surface"
    >
      <div className="flex items-baseline justify-between gap-4">
        <span className="font-serif text-lg font-semibold tracking-tight text-foreground">
          {mountName(mount, lang)}
        </span>
        <span
          aria-hidden
          className="text-subtle opacity-0 transition-opacity group-hover:opacity-100"
        >
          →
        </span>
      </div>
      {mount.description && <p className="mt-1 text-sm text-muted">{mount.description}</p>}
    </a>
  );
}
