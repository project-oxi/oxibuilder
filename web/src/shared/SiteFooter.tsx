import { useQuery } from "@tanstack/react-query";
import type { ComponentType } from "react";
import { Briefcase, Code, Globe, Mail } from "lucide-react";
import { fetchManifest, fetchProfile, type Profile } from "./api";
import { Container } from "./ui/container";

interface SiteFooterProps {
  siteName: string;
}

interface SocialLink {
  href: string;
  label: string;
  Icon: ComponentType<{ className?: string }>;
  rel?: string;
}

export function SiteFooter({ siteName }: SiteFooterProps) {
  const { data: profile } = useQuery<Profile | null>({
    queryKey: ["profile"],
    queryFn: fetchProfile,
  });
  const { data: manifest } = useQuery({ queryKey: ["manifest"], queryFn: fetchManifest });

  const year = new Date().getFullYear();
  const tagline =
    profile?.tagline_en ?? profile?.tagline_ko ?? manifest?.site.tagline ?? null;

  const links: SocialLink[] = [];
  if (profile?.email) {
    links.push({ href: `mailto:${profile.email}`, label: "Email", Icon: Mail });
  }
  if (profile?.github_username) {
    links.push({
      href: `https://github.com/${profile.github_username}`,
      label: "GitHub",
      Icon: Code,
      rel: "me",
    });
  }
  if (profile?.linkedin_url) {
    links.push({ href: profile.linkedin_url, label: "LinkedIn", Icon: Briefcase });
  }
  for (const l of profile?.custom_links ?? []) {
    links.push({ href: l.url, label: l.label, Icon: Globe });
  }

  return (
    <footer className="border-t border-line bg-surface/40">
      <Container className="py-10">
        <div className="flex flex-col gap-6 sm:flex-row sm:items-start sm:justify-between">
          <div className="space-y-1">
            <p className="font-serif text-base font-semibold text-foreground">{siteName}</p>
            {tagline && <p className="max-w-sm text-sm leading-relaxed text-muted">{tagline}</p>}
          </div>

          {links.length > 0 && (
            <nav className="flex flex-wrap items-center gap-1" aria-label="Social links">
              {links.map(({ href, label, Icon, rel }) => (
                <a
                  key={href}
                  href={href}
                  rel={rel}
                  title={label}
                  aria-label={label}
                  className="inline-flex size-9 items-center justify-center rounded-md text-muted transition-colors hover:bg-raised hover:text-primary"
                >
                  <Icon className="size-4" />
                </a>
              ))}
            </nav>
          )}
        </div>

        <div className="mt-8 border-t border-line/60 pt-5 text-center text-xs text-subtle">
          © {year} {siteName} · Powered by{" "}
          <a
            href="https://github.com/project-oxi/oxibuilder"
            className="text-muted underline-offset-2 hover:text-primary hover:underline"
          >
            Oxibuilder
          </a>
        </div>
      </Container>
    </footer>
  );
}
