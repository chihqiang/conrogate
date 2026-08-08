<?php
declare(strict_types=1);

/**
 * 不稳定 HTTP 测试上游（Swoole Http 服务器）：
 * 前 N 次请求返回 503，之后转为回显 JSON（同 echo.php）。
 * 供 test-retry.sh 校验网关自动重试（首次失败 → 重试 → 成功）。
 *
 * 用法：php flaky.php [--host 127.0.0.1] [--port 0] [--fail-first N]
 *   --fail-first 前 N 次请求返回 503（默认 2）；N=0 时永不失败。
 *   --port 0 时使用随机空闲端口，实际端口打印到 stdout（start 事件，首行）。
 */

$host = '127.0.0.1';
$port = 0;
$failFirst = 2;

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
        case '--fail-first':
            $failFirst = (int)($argvv[$i + 1] ?? $failFirst);
            $i++;
            break;
        case '-h':
        case '--help':
            fwrite(STDOUT, "usage: php flaky.php [--host 127.0.0.1] [--port 0] [--fail-first N]\n");
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

$count = 0;

$server->on('request', function (Swoole\Http\Request $req, Swoole\Http\Response $resp) use (&$count, $failFirst, $server) {
    $count++;
    if ($count <= $failFirst) {
        $resp->status(503);
        $resp->header('Content-Type', 'application/json');
        $resp->header('X-Flaky-Failed-Count', (string)$count);
        $resp->end(json_encode(['error' => 'flaky upstream down', 'failed' => $count]));
        return;
    }
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
    $resp->end(json_encode($body));
});

$server->start();
