import type { Bom_Serialize, BomLine, BomTable_Serialize } from '../bindings';

/**
 * The K2 material-list layout, as a plain model (no ExcelJS here: the same model feeds the
 * worker and the tests). K2: a « Total » tab first, then one tab per roof face; the same
 * 10 columns in every tab.
 */

/** Column keys, in K2's order. Headers are `bom.columns.<key>` (French = K2's wording). */
export const COLUMNS = [
  'articleNo',
  'description',
  'quantity',
  'mass',
  'saleUnits',
  'packQty',
  'listPrice',
  'discountRate',
  'unitPrice',
  'price',
] as const;

export type ColumnKey = (typeof COLUMNS)[number];

/** Excel number format of each numeric column (`null`: text). */
export const COLUMN_FORMATS: Record<ColumnKey, string | null> = {
  articleNo: null,
  description: null,
  quantity: '0',
  mass: '0.000',
  saleUnits: '0',
  packQty: '0',
  listPrice: '#,##0.00',
  discountRate: '0%',
  unitPrice: '#,##0.00',
  price: '#,##0.00',
};

export type Cell = string | number;

export interface SheetModel {
  name: string;
  header: string[];
  rows: Cell[][];
  /** Lines written below the table (outside the K2 columns). */
  notes: string[];
}

export interface WorkbookModel {
  sheets: SheetModel[];
}

export type Translate = (key: string, params?: Record<string, unknown>) => string;

/** Excel sheet names: ≤ 31 characters, none of `[]:*?/\`, unique. */
export function sheetNames(titles: readonly string[]): string[] {
  const used = new Set<string>();
  return titles.map((title) => {
    const clean = title.replace(/[[\]:*?/\\]/g, '-').trim() || 'Sheet';
    let name = clean.slice(0, 31);
    for (let n = 2; used.has(name.toLowerCase()); n++) {
      const suffix = ` (${String(n)})`;
      name = clean.slice(0, 31 - suffix.length) + suffix;
    }
    used.add(name.toLowerCase());
    return name;
  });
}

/** Description in the UI language, else the manufacturer's label. */
export function description(line: BomLine, lang: string): string {
  return line.descriptions[lang] ?? line.label;
}

function row(line: BomLine, lang: string): Cell[] {
  return [
    line.articleNo,
    description(line, lang),
    line.quantity,
    line.massKg,
    line.saleUnits,
    line.packQty,
    line.listPrice,
    line.discountRate,
    line.unitPrice,
    line.price,
  ];
}

/** The workbook of a bill of materials, in the UI language. */
export function buildWorkbookModel(bom: Bom_Serialize, lang: string, t: Translate): WorkbookModel {
  const header = COLUMNS.map((c) => t(`bom.columns.${c}`));
  const tables: BomTable_Serialize[] = [bom.total, ...bom.faces];
  const names = sheetNames([t('bom.total'), ...bom.faces.map((f) => f.title)]);
  return {
    sheets: tables.map((table, i) => ({
      name: names[i] ?? table.title,
      header,
      rows: table.lines.map((l) => row(l, lang)),
      notes:
        i === 0
          ? bom.savedBars.map((s) =>
              t('bom.savedBarsNote', {
                articleNo: s.articleNo,
                perFaces: s.perFaces,
                project: s.project,
                saved: s.perFaces - s.project,
              }),
            )
          : [],
    })),
  };
}
