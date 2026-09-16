import { Briefcase, Code, Globe, Mail } from "lucide-react";

import { Markdown } from "../../shared/Markdown";

export interface ProfileEducation {
  institution: string | null;
  degree: string | null;
  field: string | null;
  start_year: number | null;
  end_year: number | null;
}

export interface ProfileCustomLink {
  label: string;
  url: string;
  icon?: string | null;
}

export interface ProfileData {
  display_name: string;
  tagline_ko: string | null;
  tagline_en: string | null;
  avatar_url: string | null;
  bio_ko: string | null;
  bio_en: string | null;
  email: string | null;
  github_username: string | null;
  linkedin_url: string | null;
  education: ProfileEducation[];
  custom_links: ProfileCustomLink[];
  updated_at: string;
}

interface ProfileViewProps {
  profile: ProfileData;
  language: "ko" | "en";
}

function pick<T>(lang: "ko" | "en", ko: T | null, en: T | null): T | null {
  return lang === "ko" ? ko ?? en : en ?? ko;
}

/**
 * Identity page in the lobby's editorial language: one centered identity
 * block, then content sections on the open canvas. Empty sections render
 * nothing — never an empty box.
 */
export function ProfileView({ profile, language }: ProfileViewProps) {
  const tagline = pick(language, profile.tagline_ko, profile.tagline_en);
  const bio = pick(language, profile.bio_ko, profile.bio_en);
  const avatarUrl = profile.avatar_url;
  const initial = profile.display_name.trim().charAt(0).toUpperCase();

  const contacts: { href: string; label: string; Icon: typeof Mail; rel?: string }[] = [];
  if (profile.email) {
    contacts.push({ href: `mailto:${profile.email}`, label: profile.email, Icon: Mail });
  }
  if (profile.github_username) {
    contacts.push({
      href: `https://github.com/${profile.github_username}`,
      label: "GitHub",
      Icon: Code,
      rel: "me",
    });
  }
  if (profile.linkedin_url) {
    contacts.push({ href: profile.linkedin_url, label: "LinkedIn", Icon: Briefcase });
  }

  return (
    <article className="space-y-12">
      <header className="flex flex-col items-center gap-5 pt-8 text-center">
        {avatarUrl ? (
          <img
            src={avatarUrl}
            alt={profile.display_name}
            className="size-24 rounded-full border border-line object-cover"
          />
        ) : (
          <div
            aria-hidden
            className="flex size-24 items-center justify-center rounded-full border border-line bg-surface font-serif text-3xl font-semibold text-muted"
          >
            {initial || "?"}
          </div>
        )}
        <div className="space-y-1.5">
          <h1 className="font-serif text-4xl font-bold tracking-tight text-foreground">
            {profile.display_name}
          </h1>
          {tagline && <p className="text-base text-muted">{tagline}</p>}
        </div>
        {contacts.length > 0 && (
          <nav
            className="flex flex-wrap justify-center gap-x-5 gap-y-1.5 text-sm"
            aria-label={language === "ko" ? "연락처" : "Contact"}
          >
            {contacts.map(({ href, label, Icon, rel }) => (
              <a
                key={href}
                href={href}
                rel={rel}
                className="inline-flex items-center gap-1.5 text-muted transition-colors hover:text-primary"
              >
                <Icon className="size-3.5" />
                {label}
              </a>
            ))}
          </nav>
        )}
      </header>

      {bio && (
        <section className="markdown-lg mx-auto max-w-2xl">
          <Markdown source={bio} />
        </section>
      )}

      {profile.custom_links.length > 0 && (
        <section className="mx-auto max-w-2xl space-y-3">
          <h2 className="text-xs font-medium uppercase tracking-widest text-subtle">
            {language === "ko" ? "링크" : "Links"}
          </h2>
          <ul className="divide-y divide-line border-y border-line">
            {profile.custom_links.map((l) => (
              <li key={l.url}>
                <a
                  href={l.url}
                  className="flex items-baseline justify-between gap-4 py-3.5 text-sm no-underline transition-colors hover:text-primary"
                >
                  <span className="font-medium text-foreground">{l.label}</span>
                  <span className="truncate text-xs text-subtle">
                    {l.url.replace(/^https?:\/\//, "").replace(/\/$/, "")}
                  </span>
                </a>
              </li>
            ))}
          </ul>
        </section>
      )}

      {profile.education.length > 0 && (
        <section className="mx-auto max-w-2xl space-y-3">
          <h2 className="text-xs font-medium uppercase tracking-widest text-subtle">
            {language === "ko" ? "학력" : "Education"}
          </h2>
          <ul className="divide-y divide-line border-y border-line">
            {profile.education.map((e, i) => (
              <li key={i} className="py-3.5 text-sm">
                <span className="font-medium text-foreground">{e.institution}</span>
                {(e.degree || e.field) && (
                  <span className="text-muted">
                    {" "}— {[e.degree, e.field].filter(Boolean).join(", ")}
                  </span>
                )}
                {(e.start_year || e.end_year) && (
                  <span className="tabular-nums text-subtle">
                    {" "}({e.start_year ?? "?"}–{e.end_year ?? (language === "ko" ? "현재" : "present")})
                  </span>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}
    </article>
  );
}
