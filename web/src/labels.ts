/** Chinese labels for the codes the API stores and returns; an unknown code shows as is. */
const STATUS: Record<string, string> = { success: '成功', skipped: '跳过', failed: '失败' };

const STEP: Record<string, string> = {
  site: '站点',
  exists: '已存在',
  daily_limit: '每日上限',
  cancelled: '取消',
  download: '下载',
  parse: '解析',
  verify: '核对',
  layout: '文件布局',
  add: '加种',
  added: '已加入',
  link: '硬链接',
};

const EVIDENCE: Record<string, string> = { pieces_equal: 'pieces 完全相同', files_equal: '文件一致、pieces 不同' };

const IMPORT_OUTCOME: Record<string, string> = {
  candidate: '可加入',
  seeding: '已在做种',
  partial: '部分匹配',
  none: '没对上',
  invalid: '文件有误',
};

export const statusLabel = (code: string) => STATUS[code] ?? code;
export const evidenceLabel = (code: string) => EVIDENCE[code] ?? code;
export const importOutcomeLabel = (code: string) => IMPORT_OUTCOME[code] ?? code;
export const stepLabel = (code: string) => STEP[code] ?? code;
