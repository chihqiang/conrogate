<?php
declare(strict_types=1);

/**
 * WebSocket 测试工具（Swoole）：echo 服务器 + 客户端。
 *
 * 服务器：文本/二进制帧原样回显（echo: 前缀），供 test-ws-svc.sh 经网关隧道验证。
 *   php ws.php [--host 127.0.0.1] [--port 0]
 *     --port 0 时使用随机空闲端口，实际端口打印到 stdout（start 事件，首行）。
 *
 * 客户端：连接 + 握手（101/Sec-WebSocket-Accept 校验）+ 回显匹配校验。
 *   php ws.php --client ws://host:port/path [--message MSG]
 */

$host = '127.0.0.1';
$port = 0;
$client = null;
$message = 'hello-from-conrogate-gateway';

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
        case '--client':
            $client = $argvv[$i + 1] ?? null;
            $i++;
            break;
        case '--message':
            $message = $argvv[$i + 1] ?? $message;
            $i++;
            break;
        case '-h':
        case '--help':
            fwrite(STDOUT, "usage: php ws.php [--host 127.0.0.1] [--port 0] | --client ws://host:port/path [--message MSG]\n");
            exit(0);
    }
}

// ── 客户端模式 ──
if ($client !== null) {
    $result = 1;
    Swoole\Coroutine\run(function () use (&$result, $client, $message) {
        $parts = parse_url($client);
        $cHost = $parts['host'] ?? '127.0.0.1';
        $cPort = (int)($parts['port'] ?? 80);
        $cPath = ($parts['path'] ?? '') . (isset($parts['query']) ? '?' . $parts['query'] : '');
        $cPath = $cPath !== '' ? $cPath : '/';

        $cli = new Swoole\Coroutine\Http\Client($cHost, $cPort);
        $cli->set(['timeout' => 10]);

        if (!$cli->upgrade($cPath)) {
            fwrite(STDERR, "[FAIL] 握手失败（HTTP {$cli->statusCode}）\n");
        } elseif (!$cli->push($message, WEBSOCKET_OPCODE_TEXT)) {
            fwrite(STDERR, "[FAIL] 发送失败\n");
        } else {
            $frame = $cli->recv();
            $data = is_object($frame) ? $frame->data : '';
            if ($data === 'echo:' . $message) {
                fwrite(STDOUT, "[OK] 握手成功（101），回显校验通过：$data\n");
                $result = 0;
            } else {
                fwrite(STDERR, "[FAIL] 回显不匹配：$data\n");
            }
        }
        $cli->close();
    });
    exit($result);
}

// ── 服务器模式 ──
$server = new Swoole\WebSocket\Server($host, $port, SWOOLE_PROCESS);
$server->set([
    'daemonize' => false,
    'worker_num' => 1,
    'log_file' => '/dev/null',
]);

// master 进程启动完成：打印实际监听端口（首行），供外部脚本捕获
$server->on('start', function (Swoole\WebSocket\Server $s) {
    fwrite(STDOUT, $s->port . "\n");
    fflush(STDOUT);
});

$server->on('Message', function (Swoole\WebSocket\Server $s, Swoole\WebSocket\Frame $frame) {
    if ($frame->opcode === WEBSOCKET_OPCODE_TEXT || $frame->opcode === WEBSOCKET_OPCODE_BINARY) {
        $s->push($frame->fd, 'echo:' . $frame->data, $frame->opcode);
    }
});

$server->start();
