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

/** Known extension → icon map. Unknown extensions fall back to Sparkles. */
export const EXT_ICONS: Record<string, ComponentType<LucideProps>> = {
  blog: PenLine,
  projects: FolderGit2,
  links: Hash,
  movies: Film,
  books: BookOpen,
  novels: BookOpen,
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
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="h-28 animate-pulse rounded-lg bg-surface" />
          ))}
        </div>
      </>
    );
  }

  const hero = (
    <header className="mb-14 text-center">
      <h1 className="font-serif text-5xl font-bold tracking-tight text-foreground sm:text-6xl">
        {manifest.site.name}
      </h1>
      {manifest.site.tagline && (
        <p className="mt-3 text-sm text-subtle">{manifest.site.tagline}</p>
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
        <div className="overflow-hidden rounded-lg border border-line bg-surface divide-y divide-line">
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
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
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
      </div>
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
  const Icon = EXT_ICONS[ext.id] ?? Sparkles;
  const name = displayName(ext, lang);

  return (
    <Link
      to={`/${ext.id}`}
      data-extension={ext.id}
      style={floating && delay ? { animationDelay: `${-delay * 2}s` } : undefined}
      className={cn(
        "group relative flex flex-col gap-4 rounded-lg border border-line bg-surface p-5 shadow-sm",
        "transition-[transform,box-shadow,border-color] duration-200 ease-out",
        "hover:-translate-y-0.5 hover:shadow-md hover:border-primary/40",
        "motion-reduce:transform-none motion-reduce:transition-none",
        floating && "animate-[lobby-float_6s_ease-in-out_infinite] motion-reduce:animate-none",
      )}
    >
      <div className="flex size-11 items-center justify-center rounded-md bg-primary/10 text-primary transition-colors group-hover:bg-primary/15">
        <Icon className="size-5" />
      </div>
      <div className="space-y-0.5">
        <h2 className="font-serif text-lg font-semibold leading-tight tracking-tight text-foreground">
          {name}
        </h2>
        <p className="text-sm text-subtle">/{ext.id}</p>
      </div>
      <span
        className="absolute right-4 top-4 text-subtle opacity-0 transition-opacity duration-200 group-hover:opacity-100"
        aria-hidden
      >
        →
      </span>
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
  const Icon = EXT_ICONS[ext.id] ?? Sparkles;
  const name = displayName(ext, lang);

  return (
    <Link
      to={`/${ext.id}`}
      data-extension={ext.id}
      className="group flex items-center gap-3 px-4 py-3 transition-colors hover:bg-canvas"
    >
      <div className="flex size-9 items-center justify-center rounded-md bg-primary/10 text-primary">
        <Icon className="size-4" />
      </div>
      <span className="font-serif text-base font-medium text-foreground">
        {name}
      </span>
      <span className="text-sm text-subtle">/{ext.id}</span>
      <span className="ml-auto text-subtle opacity-0 transition-opacity group-hover:opacity-100">
        →
      </span>
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
      className={cn(
        "group relative flex flex-col gap-4 rounded-lg border border-line bg-surface p-5 shadow-sm",
        "transition-[transform,box-shadow,border-color] duration-200 ease-out",
        "hover:-translate-y-0.5 hover:shadow-md hover:border-primary/40",
      )}
    >
      <div className="flex size-11 items-center justify-center rounded-md bg-primary/10 text-primary text-xl">
        <span aria-hidden>{mount.icon ?? "🔗"}</span>
      </div>
      <div className="space-y-0.5">
        <h2 className="font-serif text-lg font-semibold leading-tight tracking-tight text-foreground">
          {mountName(mount, lang)}
        </h2>
        {mount.description && <p className="text-sm text-subtle">{mount.description}</p>}
        <p className="text-sm text-subtle">/{mount.path}</p>
      </div>
    </a>
  );
}

function MountRow({ mount, lang }: { mount: ManifestMount; lang: Lang }) {
  return (
    <a
      href={`${mount.path}/`}
      data-mount={mount.id}
      {...(mount.open_in_new_tab ? { target: "_blank", rel: "noopener" } : {})}
      className="group flex items-center gap-3 px-4 py-3 transition-colors hover:bg-canvas"
    >
      <span className="flex size-9 items-center justify-center rounded-md bg-primary/10 text-primary" aria-hidden>
        {mount.icon ?? "🔗"}
      </span>
      <span className="font-serif text-base font-medium text-foreground">
        {mountName(mount, lang)}
      </span>
      <span className="text-sm text-subtle">/{mount.path}</span>
      <span className="ml-auto text-subtle opacity-0 transition-opacity group-hover:opacity-100">→</span>
    </a>
  );
}
