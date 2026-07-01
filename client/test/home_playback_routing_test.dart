import 'package:drift/drift.dart' show Value;
import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/api/api_client.dart';
import 'package:sunflower/core/api/sunflower_api.dart';
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/player/playback_feedback_recorder.dart';
import 'package:sunflower/features/home/home_screen.dart';

void main() {
  late SunflowerDatabase db;

  setUp(() {
    db = SunflowerDatabase.forTesting(NativeDatabase.memory());
  });

  tearDown(() async {
    await db.close();
  });

  test('home playback routing uses local playback for local sections',
      () async {
    final section = sectionOf(kind: 'local_quick_picks');
    final item = itemOf(mediaId: 'yt:downloaded-later', source: 'yt');

    expect(await shouldPlayHomeItemLocally(section, item, db), isTrue);
  });

  test('home playback routing uses local playback for non-yt media ids',
      () async {
    final section = sectionOf(kind: 'quick_picks');
    final item = itemOf(mediaId: 'local:one', source: 'local');

    expect(await shouldPlayHomeItemLocally(section, item, db), isTrue);
  });

  test(
      'home playback routing starts a server queue for non-downloaded yt items',
      () async {
    final section = sectionOf(kind: 'quick_picks');
    final item = itemOf(mediaId: 'yt:remote', source: 'yt');

    expect(await shouldPlayHomeItemLocally(section, item, db), isFalse);
  });

  test('home playback routing uses local playback for downloaded yt items',
      () async {
    await db.completeDownload(
      DownloadedTracksCompanion.insert(
        mediaId: 'yt:downloaded',
        localPath: '/data/downloads/downloaded.audio',
        bytes: const Value(128),
      ),
    );
    final section = sectionOf(kind: 'quick_picks');
    final item = itemOf(mediaId: 'yt:downloaded', source: 'yt');

    expect(await shouldPlayHomeItemLocally(section, item, db), isTrue);
  });

  test('recordHomeImpression marks local event synced after remote success',
      () async {
    final feedback = _RecordingFeedbackClient();
    final local = _RecordingLocalRecommendations();
    final section = sectionOf(kind: 'quick_picks');
    final item = itemOf(mediaId: 'yt:shown', source: 'yt');

    await recordHomeImpression(
      recommendationFeedback: feedback,
      localRecommendations: local,
      section: section,
      item: item,
      index: 7,
    );

    expect(local.impressions.single.mediaId, 'yt:shown');
    expect(local.impressions.single.position, 7);
    expect(feedback.impressions.single, {
      'section_id': 'quick_picks',
      'source': 'yt',
      'seed_id': '',
      'media_id': 'yt:shown',
      'position': 7,
    });
    expect(feedback.idempotencyKeys, [_RecordingLocalRecommendations.eventId]);
    expect(local.markedSynced, [_RecordingLocalRecommendations.eventId]);
  });

  test('recordHomeImpression leaves local event unsynced after remote failure',
      () async {
    final feedback = _RecordingFeedbackClient(failImpressions: true);
    final local = _RecordingLocalRecommendations();

    await recordHomeImpression(
      recommendationFeedback: feedback,
      localRecommendations: local,
      section: sectionOf(kind: 'quick_picks'),
      item: itemOf(mediaId: 'yt:shown', source: 'yt'),
      index: 0,
    );

    expect(local.impressions.single.mediaId, 'yt:shown');
    expect(feedback.idempotencyKeys, [_RecordingLocalRecommendations.eventId]);
    expect(local.markedSynced, isEmpty);
  });
}

HomeSection sectionOf({required String kind}) {
  return HomeSection(
    id: kind,
    title: kind,
    kind: kind,
    items: const [],
  );
}

HomeItem itemOf({required String mediaId, required String source}) {
  return HomeItem(
    mediaId: mediaId,
    title: mediaId,
    source: source,
  );
}

class _RecordingFeedbackClient implements RecommendationFeedbackClient {
  _RecordingFeedbackClient({this.failImpressions = false});

  final bool failImpressions;
  final impressions = <Map<String, dynamic>>[];
  final idempotencyKeys = <String?>[];

  @override
  Future<String> like(
    String mediaId, {
    required bool liked,
    DateTime? occurredAt,
    String? idempotencyKey,
  }) {
    throw UnimplementedError();
  }

  @override
  Future<String?> logImpressions(
    List<Map<String, dynamic>> impressions, {
    String? idempotencyKey,
  }) async {
    idempotencyKeys.add(idempotencyKey);
    this.impressions.addAll(impressions);
    if (failImpressions) {
      throw StateError('remote down');
    }
    return idempotencyKey;
  }

  @override
  Future<String> playbackEvent({
    required String kind,
    required String mediaId,
    required String queueId,
    required DateTime occurredAt,
    int totalPlayedMs = 0,
    int durationMs = 0,
    String reason = '',
    String? idempotencyKey,
  }) {
    throw UnimplementedError();
  }
}

class _RecordingLocalRecommendations implements LocalRecommendationRecorder {
  static const eventId = '018f3f27-0000-7000-8000-000000000321';

  final impressions = <_RecordedImpression>[];
  final markedSynced = <String>[];

  @override
  Future<void> markFeedbackQueued(List<String> eventIds) async {
    markedSynced.addAll(eventIds);
  }

  @override
  Future<String?> recordImpression({
    required String sectionId,
    required String source,
    required String seedId,
    required String mediaId,
    required int position,
    required DateTime occurredAt,
    String? eventId,
  }) async {
    impressions.add(
      _RecordedImpression(
        sectionId: sectionId,
        source: source,
        seedId: seedId,
        mediaId: mediaId,
        position: position,
      ),
    );
    return eventId ?? _RecordingLocalRecommendations.eventId;
  }

  @override
  Future<String?> recordCompletion({
    required String mediaId,
    required String queueId,
    required int totalPlayedMs,
    required int durationMs,
    required DateTime occurredAt,
    String? eventId,
  }) {
    throw UnimplementedError();
  }

  @override
  Future<String?> recordPreference({
    required String mediaId,
    required bool liked,
    required DateTime occurredAt,
    String? eventId,
  }) {
    throw UnimplementedError();
  }

  @override
  Future<String?> recordSongPlayback(
    Song song, {
    required String queueId,
    required DateTime occurredAt,
  }) {
    throw UnimplementedError();
  }

  @override
  Future<String?> recordStreamPlayback(
    ResolvedStream stream, {
    required String queueId,
    required DateTime occurredAt,
  }) {
    throw UnimplementedError();
  }
}

class _RecordedImpression {
  const _RecordedImpression({
    required this.sectionId,
    required this.source,
    required this.seedId,
    required this.mediaId,
    required this.position,
  });

  final String sectionId;
  final String source;
  final String seedId;
  final String mediaId;
  final int position;
}
