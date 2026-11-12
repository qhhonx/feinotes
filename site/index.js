// quiet.md 不再有官网：所有访问跳转到 GitHub 仓库。
// 部署：在 site/ 目录下 `npx wrangler@4 deploy`。
const TARGET = "https://github.com/s1dashu/quiet-md";

export default {
  fetch() {
    // 302 临时跳转，保留日后恢复官网的余地（要永久可改成 301）。
    return Response.redirect(TARGET, 302);
  },
};
