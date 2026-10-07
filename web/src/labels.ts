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
};

export const statusLabel = (code: string) => STATUS[code] ?? code;
export const stepLabel = (code: string) => STEP[code] ?? code;
