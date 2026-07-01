import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/api/sunflower_api.dart';

void main() {
  test('wire models accept JSON numbers for integer fields', () {
    final song = Song.fromJson(<String, dynamic>{
      'media_id': 'local:one',
      'source_type': 'local',
      'title': 'One',
      'duration_ms': 123000.0,
      'album_id': null,
      'artist_name': 'Artist',
      'album_title': 'Album',
      'has_art': true,
    });
    expect(song.durationMs, 123000);

    final stream = ResolvedStream.fromJson(<String, dynamic>{
      'media_id': 'yt:one',
      'source': 'youtube',
      'stream_url': 'https://stream.example/audio',
      'duration_ms': 240000.0,
    });
    expect(stream.durationMs, 240000);

    final queueItem = QueueItem.fromJson(<String, dynamic>{
      'media_id': 'yt:two',
      'title': 'Two',
      'duration_ms': 180000.0,
    });
    expect(queueItem.durationMs, 180000);

    final next = NextResponse.fromJson(<String, dynamic>{
      'queue_id': 'queue-1',
      'position': 2.0,
      'current': <String, dynamic>{
        'media_id': 'yt:one',
        'source': 'youtube',
        'stream_url': 'https://stream.example/audio',
        'duration_ms': 240000.0,
      },
      'lookahead': <dynamic>[
        <String, dynamic>{
          'media_id': 'yt:two',
          'duration_ms': 180000.0,
          'source': 'youtube',
          'stream_url': 'https://stream.example/next',
          'stream_expires_at': '2026-07-01T01:00:00Z',
          'mime_type': 'audio/webm',
        },
      ],
      'continuation': 'qc_next',
      'automix': <dynamic>[
        <String, dynamic>{
          'media_id': 'yt:auto',
          'duration_ms': 120000.0,
        },
      ],
      'queue_version': 7.0,
      'has_more': true,
    });
    expect(next.position, 2);
    expect(next.current?.durationMs, 240000);
    expect(next.lookahead.single.durationMs, 180000);
    expect(next.lookahead.single.resolvedStream?.streamUrl,
        'https://stream.example/next');
    expect(next.lookahead.single.resolvedStream?.expiresAt,
        DateTime.utc(2026, 7, 1, 1));
    expect(next.continuation, 'qc_next');
    expect(next.automix.single.mediaId, 'yt:auto');
    expect(next.queueVersion, 7);

    final queue = QueueResponse.fromJson(<String, dynamic>{
      'queue_id': 'queue-1',
      'seed_kind': 'song',
      'version': 7.0,
      'items': <dynamic>[
        <String, dynamic>{
          'media_id': 'yt:two',
          'duration_ms': 180000.0,
        },
      ],
    });
    expect(queue.version, 7);
    expect(queue.items.single.durationMs, 180000);

    final home = HomeItem.fromJson(<String, dynamic>{
      'media_id': 'yt:home',
      'title': 'Home',
      'source': 'yt',
      'duration_ms': 90000.0,
    });
    expect(home.durationMs, 90000);

    final searchSong = SearchSong.fromJson(<String, dynamic>{
      'media_id': 'yt:search',
      'source': 'yt',
      'title': 'Search',
      'duration_ms': 60000.0,
    });
    expect(searchSong.durationMs, 60000);

    final playlist = Playlist.fromJson(<String, dynamic>{
      'id': 'playlist-1',
      'title': 'Playlist',
      'version': 3.0,
      'items': <dynamic>[
        <String, dynamic>{
          'media_id': 'yt:item',
          'title': 'Item',
          'source': 'yt',
          'duration_ms': 45000.0,
        },
      ],
    });
    expect(playlist.version, 3);
    expect(playlist.items.single.durationMs, 45000);

    final hash = SongHash.fromJson(<String, dynamic>{
      'media_id': 'local:one',
      'sha256': 'abc123',
      'bytes': 4096.0,
    });
    expect(hash.bytes, 4096);
  });

  test('nullable song duration remains nullable', () {
    final song = Song.fromJson(<String, dynamic>{
      'media_id': 'local:no-duration',
      'source_type': 'local',
      'title': 'No Duration',
      'duration_ms': null,
      'album_id': null,
      'artist_name': '',
      'album_title': '',
      'has_art': false,
    });

    expect(song.durationMs, isNull);
  });

  test('metadata-only lookahead remains unresolved for legacy servers', () {
    final next = NextResponse.fromJson(<String, dynamic>{
      'queue_id': 'queue-legacy',
      'position': 0,
      'current': null,
      'lookahead': <dynamic>[
        <String, dynamic>{
          'media_id': 'yt:legacy',
          'title': 'Legacy',
          'duration_ms': 180000,
        },
      ],
      'has_more': false,
    });

    expect(next.lookahead.single.mediaId, 'yt:legacy');
    expect(next.lookahead.single.resolvedStream, isNull);
    expect(next.queueVersion, 0);
  });
}
