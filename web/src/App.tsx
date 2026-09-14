import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
import { BrowserRouter, Link, Route, Routes } from "react-router";
import { lazy, Suspense, useEffect } from "react";
import { Search } from "lucide-react";

import { fetchManifest } from "./shared/api";
import { Lobby } from "./lobby/Lobby";
import { HubLobby } from "./lobby/HubLobby";
import { ProfilePage } from "./extensions/profile/ProfilePage";
import { LanguageProvider } from "./shared/language";
import type { Lang } from "./shared/language";
import { LangToggle } from "./shared/LangToggle";
import { ThemeToggle } from "./shared/ThemeToggle";
import { applyServerTheme, applyThemeMode, getConsoleAppearance } from "./shared/theme";
import { Button } from "./shared/ui/button";
import { Container } from "./shared/ui/container";
import { Skeleton } from "./shared/ui/skeleton";
import { AssetResolverProvider } from "./shared/asset-context";
import { SiteFooter } from "./shared/SiteFooter";
import { EditorialShell } from "./shell/EditorialShell";

const queryClient = new QueryClient();

// Lazy chunks — 확장 하나 = lazy route chunk 하나 (doc/01 §1.5 코드 스플리팅).
const BlogListPage = lazy(() =>
  import("./extensions/blog/BlogListPage").then((m) => ({ default: m.BlogListPage })),
);
const BlogPostPage = lazy(() =>
  import("./extensions/blog/BlogPostPage").then((m) => ({ default: m.BlogPostPage })),
);
const BlogSeriesPage = lazy(() =>
  import("./extensions/blog/BlogSeriesPage").then((m) => ({ default: m.BlogSeriesPage })),
);
const ProjectsListPage = lazy(() =>
  import("./extensions/projects/ProjectsListPage").then((m) => ({ default: m.ProjectsListPage })),
);
const ProjectDetailPage = lazy(() =>
  import("./extensions/projects/ProjectDetailPage").then((m) => ({ default: m.ProjectDetailPage })),
);
const LinksPage = lazy(() =>
  import("./extensions/links/LinksPage").then((m) => ({ default: m.LinksPage })),
);
const SearchPage = lazy(() =>
  import("./search/SearchPage").then((m) => ({ default: m.SearchPage })),
);
const NovelsPage = lazy(() =>
  import("./extensions/novels/NovelsPage").then((m) => ({ default: m.NovelsPage })),
);
const MoviesPage = lazy(() =>
  import("./extensions/movies/MoviesPage").then((m) => ({ default: m.MoviesPage })),
);
const MoviesStatsPage = lazy(() =>
  import("./extensions/movies/MoviesStatsPage").then((m) => ({ default: m.MoviesStatsPage })),
);
const MovieDetailPage = lazy(() =>
  import("./extensions/movies/MovieDetailPage").then((m) => ({ default: m.MovieDetailPage })),
);
const BooksPage = lazy(() =>
  import("./extensions/books/BooksPage").then((m) => ({ default: m.BooksPage })),
);
const BooksStatsPage = lazy(() =>
  import("./extensions/books/BooksStatsPage").then((m) => ({ default: m.BooksStatsPage })),
);
const ScrapsPage = lazy(() =>
  import("./extensions/scraps/ScrapsPage").then((m) => ({ default: m.ScrapsPage })),
);
const ActivityPage = lazy(() =>
  import("./extensions/activity/ActivityPage").then((m) => ({ default: m.ActivityPage })),
);

function PageFallback() {
  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {Array.from({ length: 6 }).map((_, i) => (
        <Skeleton key={i} className="h-28" />
      ))}
    </div>
  );
}

type Layout = "shell" | "editorial";

function SiteRoutes({ layout }: { layout: Layout }) {
  return (
    <Routes>
      <Route
        path="/"
        element={layout === "editorial" ? <HubLobby /> : <Lobby />}
      />
      <Route path="/profile" element={<ProfilePage />} />
      <Route
        path="/blog"
        element={
          <Suspense fallback={<PageFallback />}>
            <BlogListPage />
          </Suspense>
        }
      />
      <Route
        path="/blog/series/:slug"
        element={
          <Suspense fallback={<PageFallback />}>
            <BlogSeriesPage />
          </Suspense>
        }
      />
      <Route
        path="/blog/:slug"
        element={
          <Suspense fallback={<PageFallback />}>
            <BlogPostPage />
          </Suspense>
        }
      />
      <Route
        path="/projects"
        element={
          <Suspense fallback={<PageFallback />}>
            <ProjectsListPage />
          </Suspense>
        }
      />
      <Route
        path="/projects/:slug"
        element={
          <Suspense fallback={<PageFallback />}>
            <ProjectDetailPage />
          </Suspense>
        }
      />
      <Route
        path="/links"
        element={
          <Suspense fallback={<PageFallback />}>
            <LinksPage />
          </Suspense>
        }
      />
      <Route
        path="/novels"
        element={
          <Suspense fallback={<PageFallback />}>
            <NovelsPage />
          </Suspense>
        }
      />
      <Route
        path="/movies"
        element={
          <Suspense fallback={<PageFallback />}>
            <MoviesPage />
          </Suspense>
        }
      />
      <Route
        path="/movies/stats"
        element={
          <Suspense fallback={<PageFallback />}>
            <MoviesStatsPage />
          </Suspense>
        }
      />
      <Route
        path="/movies/:slug"
        element={
          <Suspense fallback={<PageFallback />}>
            <MovieDetailPage />
          </Suspense>
        }
      />
      <Route
        path="/books"
        element={
          <Suspense fallback={<PageFallback />}>
            <BooksPage />
          </Suspense>
        }
      />
      <Route
        path="/books/stats"
        element={
          <Suspense fallback={<PageFallback />}>
            <BooksStatsPage />
          </Suspense>
        }
      />
      <Route
        path="/scraps"
        element={
          <Suspense fallback={<PageFallback />}>
            <ScrapsPage />
          </Suspense>
        }
      />
      <Route
        path="/activity"
        element={
          <Suspense fallback={<PageFallback />}>
            <ActivityPage />
          </Suspense>
        }
      />
      <Route
        path="/search"
        element={
          <Suspense fallback={<PageFallback />}>
            <SearchPage />
          </Suspense>
        }
      />
      <Route path="*" element={<p className="text-subtle">404</p>} />
    </Routes>
  );
}

function Shell({ children }: { children: React.ReactNode }) {
  useEffect(() => {
    void applyServerTheme().then((def) => {
      // Public site reflects the selected theme's light/dark mode — unless the
      // visitor already picked an explicit console appearance, which wins.
      // This is the public shell only; the admin console never mutates
      // .dark through this path.
      if (!def) return;
      const saved = getConsoleAppearance();
      applyThemeMode(saved === "system" ? def.mode : saved);
    });
  }, []);
  const { data: manifest } = useQuery({ queryKey: ["manifest"], queryFn: fetchManifest });
  const defaultLang: Lang = manifest?.site.default_lang === "en" ? "en" : "ko";
  const siteName = manifest?.site.name ?? "Oxibuilder";

  return (
    <LanguageProvider key={defaultLang} defaultLang={defaultLang}>
      <div className="flex min-h-screen flex-col">
        <header className="bg-canvas/80 supports-[backdrop-filter]:bg-canvas/60 sticky top-0 z-40 border-b border-line backdrop-blur-md">
          <Container className="flex h-14 items-center justify-between gap-4">
            <Link
              to="/"
              className="group inline-flex items-baseline gap-1.5 font-serif text-lg font-semibold tracking-tight text-foreground transition-colors hover:text-primary"
            >
              {siteName}
              <span className="size-1.5 self-center rounded-full bg-primary transition-transform group-hover:scale-125" aria-hidden />
            </Link>
            <nav className="flex items-center gap-1">
              <Button variant="ghost" size="sm" asChild>
                <Link to="/search">
                  <Search />
                  <span className="hidden sm:inline">
                    {defaultLang === "en" ? "Search" : "검색"}
                  </span>
                </Link>
              </Button>
              <LangToggle />
              <ThemeToggle />
            </nav>
          </Container>
        </header>

        <main className="flex-1 py-8">
          <Container>{children}</Container>
        </main>

        <SiteFooter siteName={siteName} />
      </div>
    </LanguageProvider>
  );
}

export function App() {
  const layout = (document.documentElement.dataset.layout ?? "shell") as Layout;
  return (
    <QueryClientProvider client={queryClient}>
      <AssetResolverProvider mode="public">
        <BrowserRouter>
          {layout === "editorial" ? (
            <EditorialShell>
              <SiteRoutes layout="editorial" />
            </EditorialShell>
          ) : (
            <Shell>
              <SiteRoutes layout="shell" />
            </Shell>
          )}
        </BrowserRouter>
      </AssetResolverProvider>
    </QueryClientProvider>
  );
}