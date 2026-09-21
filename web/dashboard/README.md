# conrogate-dashboard

Conrogate 网关控制面 Web 管理台（独立部署的可选前端组件）。不依赖后端打包，通过控制面 REST API 对接。

## 功能

- **路由管理**：分页列表、新建/编辑（路径匹配、HTTP 方法、上游选择、Host 头）、启用切换、删除
- **上游管理**：分页列表、节点动态编辑、负载均衡算法下拉、删除确认
- **插件管理**：已安装插件列表（log/cors/auth）、启用/停用/卸载（Admin 专属）、状态筛选
- **路由插件绑定**：路由「插件」入口 → 查看/绑定/编辑/解绑插件，JSON 配置带官方插件默认模板
- **配置版本**：发布新版本（快照）、版本历史、回滚、任意两版本差异对比
- **指标洞察**：QPS 时序、延迟百分位、状态码分布、热门路由 TOP10（ECharts，支持时间范围与自动刷新）
- **节点**：数据面节点在线状态（心跳判定）与配置版本应用情况
- **审计日志**：分页查询 + 操作人/动作/资源筛选

## 技术栈

Vue 3 + TypeScript 6 + Vite 8 + Tailwind CSS v4 + Pinia + Vue Router 4 + ECharts(vue-echarts)

## 快速开始

```bash
npm install
cp .env.example .env   # 按需修改环境变量
npm run dev            # http://localhost:5173
```

生产构建：

```bash
npm run build          # = vue-tsc --noEmit && vite build
npm run preview
```

## 环境变量

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `VITE_API_BASE` | `/api/v1` | 控制面 API 基础路径（控制面默认挂载于 `/api/v1` 前缀）。走 Vite 代理时保持默认 |
| `VITE_PROXY_TARGET` | `http://127.0.0.1:9000` | 仅 dev 模式：Vite 代理转发目标（控制面监听地址） |

直接对接远程控制面时，将 `VITE_API_BASE` 设为完整 URL（如 `https://api.example.com/api/v1`），无需代理。

## 对接控制面

- 鉴权：`Authorization: Bearer <operator>:<secret>:<role>`；角色 `viewer`（只读）/ `operator`（读写）/ `admin`（全权限）。登录页输入完整 token，会实时预览角色。
- 响应信封 `{code, msg, data, trace_id}`，`code = 0` 成功；401 / `code=10002` 自动清除 token 并跳回登录页。
- 接口契约详见仓库根目录 `docs/api.md`。

## 目录结构

```
src/
├── api/        # 控制面 REST 客户端（routes/upstreams/plugins/configs/metrics/nodes/events）
├── types/      # 类型与后端 serde 枚举（protocol=web_socket 等）
├── stores/     # Pinia：auth（token/角色/权限）、toast
├── router/     # Vue Router（守卫 + redirect 回跳）
├── components/ # 通用 UI（Button/Input/Select/Badge/Card/Modal/Table/Pagination/Empty/Toast）+ layout
└── views/      # 页面视图（Login/Routes/Upstreams/Plugins/Configs/Metrics/Nodes/Audit）
```
