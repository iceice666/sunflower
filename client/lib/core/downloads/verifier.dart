import 'dart:io';

import 'package:crypto/crypto.dart';

/// Streams a file through SHA-256 without loading it into memory — used to verify
/// local-library downloads against the server-provided hash
/// (GET /library/songs/{id}/hash).
class Verifier {
  /// Returns the lowercase hex SHA-256 of the file at [path].
  Future<String> sha256OfFile(String path) async {
    final file = File(path);
    final sink = _Sink();
    final out = sha256.startChunkedConversion(sink);
    await for (final chunk in file.openRead()) {
      out.add(chunk);
    }
    out.close();
    return sink.digest.toString();
  }

  /// True if the file at [path] matches [expectedHex] (case-insensitive).
  Future<bool> verify(String path, String expectedHex) async {
    final got = await sha256OfFile(path);
    return got.toLowerCase() == expectedHex.toLowerCase();
  }
}

/// Captures the final digest from the chunked converter.
class _Sink implements Sink<Digest> {
  late Digest digest;

  @override
  void add(Digest data) => digest = data;

  @override
  void close() {}
}
