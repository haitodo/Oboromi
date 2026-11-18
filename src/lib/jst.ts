const JST_OFFSET_MS = 9 * 60 * 60 * 1000;

export function currentJstDate(): string {
  const jst = new Date(Date.now() + JST_OFFSET_MS);
  const y = jst.getUTCFullYear();
  const m = String(jst.getUTCMonth() + 1).padStart(2, '0');
  const d = String(jst.getUTCDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

export function parseYmd(ymd: string): { year: number; month: number; day: number } {
  const [y, m, d] = ymd.split('-').map(Number);
  return { year: y, month: m, day: d };
}

export function formatYmd(year: number, month: number, day: number): string {
  return `${String(year).padStart(4, '0')}-${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
}

export function shiftYmd(ymd: string, deltaDays: number): string {
  const { year, month, day } = parseYmd(ymd);
  const dt = new Date(Date.UTC(year, month - 1, day + deltaDays));
  return formatYmd(dt.getUTCFullYear(), dt.getUTCMonth() + 1, dt.getUTCDate());
}

export function shiftYearMonth(
  year: number,
  month: number,
  deltaMonths: number
): { year: number; month: number } {
  const dt = new Date(Date.UTC(year, month - 1 + deltaMonths, 1));
  return { year: dt.getUTCFullYear(), month: dt.getUTCMonth() + 1 };
}

export interface CalendarCell {
  date: string;
  day: number;
  inCurrentMonth: boolean;
}

export function buildCalendarGrid(year: number, month: number): CalendarCell[] {
  const firstDow = new Date(Date.UTC(year, month - 1, 1)).getUTCDay(); // 0=Sun..6=Sat
  const daysInMonth = new Date(Date.UTC(year, month, 0)).getUTCDate();
  const prevMonthDays = new Date(Date.UTC(year, month - 1, 0)).getUTCDate();

  const cells: CalendarCell[] = [];
  for (let i = firstDow - 1; i >= 0; i--) {
    const day = prevMonthDays - i;
    const prev = shiftYearMonth(year, month, -1);
    cells.push({
      date: formatYmd(prev.year, prev.month, day),
      day,
      inCurrentMonth: false,
    });
  }
  for (let day = 1; day <= daysInMonth; day++) {
    cells.push({
      date: formatYmd(year, month, day),
      day,
      inCurrentMonth: true,
    });
  }
  const trailing = (7 - (cells.length % 7)) % 7;
  const next = shiftYearMonth(year, month, 1);
  for (let day = 1; day <= trailing; day++) {
    cells.push({
      date: formatYmd(next.year, next.month, day),
      day,
      inCurrentMonth: false,
    });
  }
  return cells;
}

export function formatTimestamp(ms: number): string {
  const jst = new Date(ms + JST_OFFSET_MS);
  const y = jst.getUTCFullYear();
  const m = String(jst.getUTCMonth() + 1).padStart(2, '0');
  const d = String(jst.getUTCDate()).padStart(2, '0');
  const hh = String(jst.getUTCHours()).padStart(2, '0');
  const mm = String(jst.getUTCMinutes()).padStart(2, '0');
  return `${y}-${m}-${d} ${hh}:${mm}`;
}

export function formatDiaryDate(date: string, includeYear = true): string {
  const { year, month, day } = parseYmd(date);
  return `${includeYear ? `${year}年` : ''}${month}月${day}日`;
}

export function formatDiaryWeekday(date: string): string {
  const day = new Date(`${date}T00:00:00Z`).getUTCDay();
  return `${'日月火水木金土'[day]}曜日`;
}
