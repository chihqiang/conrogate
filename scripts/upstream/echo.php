<?php
declare(strict_types=1);

/**
 * HTTP 回显测试上游（Swoole Http 服务器）：
 * 对任意请求回显 JSON {method, path, query, headers, body}，
 * 并注入响应头 X-Upstream-Port（本实例实际端口）与调用方指定头。
 * 供 test-header-condition.sh / test-plugin-*.sh 校验网关路由命中、
 * 插件请求头改写等场景（通过端口/响应头区分命中了哪个上游）。
 *
 * 用法：php echo.php [--host 127.0.0.1] [--port 0] [--add-resp-header k:v]...
 *   --port 0 时使用随机空闲端口，实际端口打印到 stdout（start 事件，首行）。
 *   --add-resp-header 可叠加，用于校验 header_rewrite 的响应头 remove。
 */

$host = '127.0.0.1';
$port = 0;
$extraHeaders = [];

$argvv = $argv ?? [];
for ($i = 1, $n = count($argvv); $i < $n; $i++) {
    switch ($argvv[$i]) {
        case '--host':
            $host = $argvv[$i + 1] ?? $host;
            $i++;
            break;
        case '--port':
            $port = (int)($argvv[$i + 1] ?? $port);
            $i++;
            break;
        case '--add-resp-header':
            $extraHeaders[] = $argvv[$i + 1] ?? '';
            $i++;
            break;
        case '-h':
        case '--help':
            fwrite(STDOUT, "usage: php echo.php [--host 127.0.0.1] [--port 0] [--add-resp-header k:v]...\n");
            exit(0);
    }
}

$server = new Swoole\Http\Server($host, $port, SWOOLE_PROCESS);
$server->set([
    'worker_num' => 1,
    'log_file' => '/dev/null',
]);

// master 进程启动完成：打印实际监听端口（首行），供外部脚本捕获
$server->on('start', function (Swoole\Http\Server $s) {
    fwrite(STDOUT, $s->port . "\n");
    fflush(STDOUT);
});

$server->on('request', function (Swoole\Http\Request $req, Swoole\Http\Response $resp) use ($extraHeaders, $server) {
    $headers = [];
    foreach (($req->header ?? []) as $k => $v) {
        $headers[$k] = $v;
    }
    $body = [
        'method' => $req->server['request_method'] ?? '',
        'path' => $req->server['request_uri'] ?? '',
        'query' => $req->server['query_string'] ?? '',
        'headers' => $headers,
        'body' => $req->rawContent() === false ? '' : $req->rawContent(),
    ];
    $resp->header('Content-Type', 'application/json');
    $resp->header('X-Upstream-Port', (string)$server->port);
    foreach ($extraHeaders as $kv) {
        $pos = strpos($kv, ':');
        if ($pos !== false) {
            $resp->header(substr($kv, 0, $pos), substr($kv, $pos + 1));
        }
    }
    $resp->end(json_encode($body));
});

$server->start();
