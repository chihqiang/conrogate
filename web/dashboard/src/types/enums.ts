/**
 * 全局枚举定义。
 *
 * 所有枚举的字符串值必须与后端 Rust 契约一致（serde snake_case 序列化值），
 * 例如 `ProtocolId::WebSocket` 序列化为 `"web_socket"` 而非 `"websocket"`。
 * 若后端枚举值有变动，请同步修改本文件。
 */

/** 控制面鉴权角色（来自 token 的 `operator:secret:role` 第三段） */
export enum Role {
  Viewer = 'viewer',
  Operator = 'operator',
  Admin = 'admin',
}

export const RoleLabels: Record<Role, string> = {
  [Role.Viewer]: '只读',
  [Role.Operator]: '操作员',
  [Role.Admin]: '管理员',
}

/** 数据面支持的协议标识 */
export enum RouteProtocol {
  Http = 'http',
  WebSocket = 'web_socket',
  TcpTunnel = 'tcp_tunnel',
}

export const RouteProtocolLabels: Record<RouteProtocol, string> = {
  [RouteProtocol.Http]: 'HTTP',
  [RouteProtocol.WebSocket]: 'WebSocket',
  [RouteProtocol.TcpTunnel]: 'TCP 隧道',
}

/** 路径匹配类型（后端 `PathMatch` 为 tagged union，见 types/index.ts 的 PathMatchUnion） */
export enum PathMatchType {
  Prefix = 'prefix',
  Exact = 'exact',
  Regex = 'regex',
}

export const PathMatchTypeLabels: Record<PathMatchType, string> = {
  [PathMatchType.Prefix]: '前缀匹配',
  [PathMatchType.Exact]: '精确匹配',
  [PathMatchType.Regex]: '正则匹配',
}

/** 通用匹配操作符（Header / Query 条件） */
export enum MatchOp {
  Exact = 'exact',
  Prefix = 'prefix',
  Regex = 'regex',
  NotEmpty = 'not_empty',
}

export const MatchOpLabels: Record<MatchOp, string> = {
  [MatchOp.Exact]: '等于',
  [MatchOp.Prefix]: '前缀',
  [MatchOp.Regex]: '正则',
  [MatchOp.NotEmpty]: '非空',
}

/** HTTP 方法（路由匹配条件 methods 取值） */
export enum HttpMethod {
  Get = 'GET',
  Post = 'POST',
  Put = 'PUT',
  Delete = 'DELETE',
  Patch = 'PATCH',
  Head = 'HEAD',
  Options = 'OPTIONS',
}

export const HttpMethodLabels: Record<HttpMethod, string> = {
  [HttpMethod.Get]: 'GET',
  [HttpMethod.Post]: 'POST',
  [HttpMethod.Put]: 'PUT',
  [HttpMethod.Delete]: 'DELETE',
  [HttpMethod.Patch]: 'PATCH',
  [HttpMethod.Head]: 'HEAD',
  [HttpMethod.Options]: 'OPTIONS',
}

/** 负载均衡算法 */
export enum BalancerAlgorithm {
  RoundRobin = 'round_robin',
  WeightedRoundRobin = 'weighted_round_robin',
  LeastConnections = 'least_connections',
  ConsistentHash = 'consistent_hash',
}

export const BalancerAlgorithmLabels: Record<BalancerAlgorithm, string> = {
  [BalancerAlgorithm.RoundRobin]: '轮询',
  [BalancerAlgorithm.WeightedRoundRobin]: '加权轮询',
  [BalancerAlgorithm.LeastConnections]: '最少连接',
  [BalancerAlgorithm.ConsistentHash]: '一致性哈希',
}

/** 配置版本发布类型 */
export enum PublishType {
  Publish = 'publish',
  Rollback = 'rollback',
}

export const PublishTypeLabels: Record<PublishType, string> = {
  [PublishType.Publish]: '发布',
  [PublishType.Rollback]: '回滚',
}

/** 插件类型 */
export enum PluginKind {
  Native = 'native',
  Wasm = 'wasm',
}

export const PluginKindLabels: Record<PluginKind, string> = {
  [PluginKind.Native]: '内置',
  [PluginKind.Wasm]: 'WASM 扩展',
}

/** 插件生命周期状态 */
export enum PluginStatus {
  Installed = 'installed',
  Active = 'active',
  Disabled = 'disabled',
  Uninstalled = 'uninstalled',
}

export const PluginStatusLabels: Record<PluginStatus, string> = {
  [PluginStatus.Installed]: '已安装',
  [PluginStatus.Active]: '已启用',
  [PluginStatus.Disabled]: '已停用',
  [PluginStatus.Uninstalled]: '已卸载',
}

/** 网关事件类型（数据面上报 + 控制面记录） */
export enum GatewayEventType {
  RateLimited = 'rate_limited',
  CircuitBreakerOpen = 'circuit_breaker_open',
  UpstreamFailed = 'upstream_failed',
  UpstreamTimeout = 'upstream_timeout',
  PluginTerminate = 'plugin_terminate',
  PluginMetricIncrement = 'plugin.metric.increment',
  PluginMetricGauge = 'plugin.metric.gauge',
  PluginLog = 'plugin.log',
}

export const GatewayEventTypeLabels: Record<GatewayEventType, string> = {
  [GatewayEventType.RateLimited]: '限流',
  [GatewayEventType.CircuitBreakerOpen]: '熔断',
  [GatewayEventType.UpstreamFailed]: '上游失败',
  [GatewayEventType.UpstreamTimeout]: '上游超时',
  [GatewayEventType.PluginTerminate]: '插件终止',
  [GatewayEventType.PluginMetricIncrement]: '插件指标（计数）',
  [GatewayEventType.PluginMetricGauge]: '插件指标（计量）',
  [GatewayEventType.PluginLog]: '插件日志',
}

/** 将「值 → 中文标签」映射转为下拉框选项数组 */
export function toOptions<T extends string>(
  labels: Record<T, string>,
): { value: T; label: string }[] {
  return (Object.keys(labels) as T[]).map((value) => ({ value, label: labels[value] }))
}
