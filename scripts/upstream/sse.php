<?php
declare(strict_types=1);

/**
 * SSE 流式测试上游（Swoole Http 服务器）：
 * 对任意路径响应 text/event-stream，按固定间隔逐条写出 data: chunk-N，
 * 供 test-sse-svc.sh 验证网关是否流式透传（而非整体缓冲）。
 *
 * 用法：php sse.php [--host 127.0.0.1] [--port 0] [--count 8] [--delay-ms 250]
 *   --port 0 时使用随机空闲端口，实际端口打印到 stdout（start 事件，首行）。
 *   请求参数 ?count=N&delay_ms=M 可覆盖默认值（测试轮询用 count=1&delay_ms=0）。
 */

$host = '127.0.0.1';
$port = 0;
$count = 8;
$delayMs = 250;

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
        case '--count':
            $count = max(1, (int)($argvv[$i + 1] ?? $count));
            $i++;
            break;
        case '--delay-ms':
            $delayMs = max(0, (int)($argvv[$i + 1] ?? $delayMs));
            $i++;
            break;
        case '-h':
        case '--help':
            fwrite(STDOUT, "usage: php sse.php [--host 127.0.0.1] [--port 0] [--count 8] [--delay-ms 250]\n");
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

$server->on('request', function (Swoole\Http\Request $req, Swoole\Http\Response $resp) use ($count, $delayMs) {
    $c = isset($req->get['count']) ? max(1, (int)$req->get['count']) : $count;
    $d = isset($req->get['delay_ms']) ? max(0, (int)$req->get['delay_ms']) : $delayMs;

    $resp->header('Content-Type', 'text/event-stream; charset=utf-8');
    $resp->header('Cache-Control', 'no-cache');
    for ($i = 0; $i < $c; $i++) {
        $resp->write("data: chunk-$i\n\n");
        if ($d > 0) {
            Swoole\Coroutine::sleep($d / 1000.0);
        }
    }
    $resp->end();
});

$server->start();
