import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { ArrowLeft } from "lucide-react";

import { fetchBooks } from "../../shared/api";
import { useLanguage } from "../../shared/language";
import { PageTitle } from "../../shared/ui/page-header";
import { BarRow, ColumnChart, SummaryBand } from "../../shared/stats/StatsKit";
import { computeBookStats, statusLabel } from "../../shared/stats/computeBookStats";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="space-y-2">
      <h2 className="font-serif text-lg font-semibold text-foreground">{title}</h2>
      {children}
    </section>
  );
}

export function BooksStatsPage() {
  const { lang } = useLanguage();
  const ko = lang === "ko";
  const { data: books, isLoading } = useQuery({ queryKey: ["books"], queryFn: fetchBooks });

  const stats = useMemo(() => (books ? computeBookStats(books) : null), [books]);

  if (isLoading) return <p className="text-subtle">…</p>;
  if (!books || books.length === 0 || !stats) {
    return (
      <article className="space-y-4">
        <PageTitle>{ko ? "도서 통계" : "Book Stats"}</PageTitle>
        <p className="text-subtle">{ko ? "아직 책이 없습니다." : "No books yet."}</p>
      </article>
    );
  }

  const yearRange = stats.yearMax > 0 ? `${stats.yearMin}–${stats.yearMax}` : "—";

  return (
    <article className="space-y-10">
      <div className="flex items-center gap-3">
        <Link
          to="/books"
          className="text-subtle transition-colors hover:text-foreground"
          aria-label={ko ? "목록으로" : "Back to list"}
        >
          <ArrowLeft className="size-5" />
        </Link>
        <PageTitle>{ko ? "도서 통계" : "Book Stats"}</PageTitle>
      </div>

      <SummaryBand
        items={[
          { label: ko ? "총 권수" : "Books", value: stats.total },
          { label: ko ? "저자" : "Authors", value: stats.authorCount },
          { label: ko ? "평균 평점" : "Avg rating", value: stats.ratingMean ? stats.ratingMean.toFixed(1) : null },
          { label: ko ? "연도" : "Years", value: yearRange },
        ]}
      />

      {stats.years.length > 0 && (
        <Section title={ko ? "연도별" : "By year"}>
          <ColumnChart data={stats.years} max={Math.max(...stats.years.map((y) => y.count), 1)} />
        </Section>
      )}

      {stats.byStatus.length > 0 && (
        <Section title={ko ? "상태" : "Status"}>
          {stats.byStatus.map((s) => (
            <BarRow key={s.name} name={statusLabel(s.name, ko)} count={s.count} max={Math.max(...stats.byStatus.map((x) => x.count), 1)} />
          ))}
        </Section>
      )}

      {stats.authors.length > 0 && (
        <Section title={ko ? "저자" : "Authors"}>
          {stats.authors.map((a) => (
            <BarRow key={a.name} name={a.name} count={a.count} max={stats.authors[0].count} />
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

      {stats.categories.length > 0 && (
        <Section title={ko ? "카테고리" : "Categories"}>
          {stats.categories.map((c) => (
            <BarRow key={c.name} name={c.name} count={c.count} max={stats.categories[0].count} />
          ))}
        </Section>
      )}

      {stats.publishers.length > 0 && (
        <Section title={ko ? "출판사" : "Publishers"}>
          {stats.publishers.map((p) => (
            <BarRow key={p.name} name={p.name} count={p.count} max={stats.publishers[0].count} />
          ))}
        </Section>
      )}

      {stats.pageBuckets.length > 0 && (
        <Section title={ko ? "페이지" : "Pages"}>
          {stats.pageBuckets.map((b) => (
            <BarRow key={b.name} name={b.name} count={b.count} max={Math.max(...stats.pageBuckets.map((x) => x.count), 1)} />
          ))}
        </Section>
      )}
    </article>
  );
}
