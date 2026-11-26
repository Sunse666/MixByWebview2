# A local endpoint that records the headers of every request it gets.
#
# Used to check that a download carries the credentials the page would have sent, and
# crucially that a *retry* carries them too. httpbin echoes Referer but not Cookie, and the
# whole question here is about cookies.
param(
    [int]$Port = 18080,
    [string]$LogFile = "$env:TEMP\mix-header-log.txt"
)

$listener = New-Object System.Net.HttpListener
$listener.Prefixes.Add("http://127.0.0.1:$Port/")
$listener.Start()
"[server] listening on 127.0.0.1:$Port, log=$LogFile" | Out-File $LogFile -Encoding UTF8

# Big enough that a transfer can be interrupted part way.
$payload = New-Object byte[] (2 * 1024 * 1024)
(New-Object Random 42).NextBytes($payload)

while ($listener.IsListening) {
    try { $context = $listener.GetContext() } catch { break }
    $request = $context.Request
    $response = $context.Response

    $line = "{0} {1} referer=[{2}] cookie=[{3}]" -f `
        $request.HttpMethod, $request.Url.AbsolutePath, `
        $request.Headers['Referer'], $request.Headers['Cookie']
    $line | Out-File $LogFile -Append -Encoding UTF8

    if ($request.Url.AbsolutePath -eq "/file") {
        # A specific cookie is required, not merely any cookie: the browser keeps cookies for
        # every site it has visited, so "has a cookie" would pass on another site's jar and
        # the download would succeed without this page's credential ever being involved.
        $cookie = $request.Headers['Cookie']
        if (-not $cookie -or $cookie -notmatch 'sid=secret-value') {
            $response.StatusCode = 403
            $body = [Text.Encoding]::UTF8.GetBytes("no cookie")
            $response.OutputStream.Write($body, 0, $body.Length)
            $response.Close()
            continue
        }
        $response.ContentType = "application/octet-stream"
        $response.ContentLength64 = $payload.Length
        $response.Headers.Add("Accept-Ranges", "bytes")
        try { $response.OutputStream.Write($payload, 0, $payload.Length) } catch { }
        $response.Close()
        continue
    }

    # Anything else: a page that sets a cookie, so a tab on this origin has one.
    $response.ContentType = "text/html"
    $response.Headers.Add("Set-Cookie", "sid=secret-value; Path=/")
    $body = [Text.Encoding]::UTF8.GetBytes("<html><body>ok</body></html>")
    $response.ContentLength64 = $body.Length
    $response.OutputStream.Write($body, 0, $body.Length)
    $response.Close()
}
