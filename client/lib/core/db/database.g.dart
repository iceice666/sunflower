// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'database.dart';

// ignore_for_file: type=lint
class $LookaheadCacheTable extends LookaheadCache
    with TableInfo<$LookaheadCacheTable, LookaheadCacheData> {
  @override
  final GeneratedDatabase attachedDatabase;
  final String? _alias;
  $LookaheadCacheTable(this.attachedDatabase, [this._alias]);
  static const VerificationMeta _queueIdMeta =
      const VerificationMeta('queueId');
  @override
  late final GeneratedColumn<String> queueId = GeneratedColumn<String>(
      'queue_id', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _positionMeta =
      const VerificationMeta('position');
  @override
  late final GeneratedColumn<int> position = GeneratedColumn<int>(
      'position', aliasedName, false,
      type: DriftSqlType.int, requiredDuringInsert: true);
  static const VerificationMeta _mediaIdMeta =
      const VerificationMeta('mediaId');
  @override
  late final GeneratedColumn<String> mediaId = GeneratedColumn<String>(
      'media_id', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _titleMeta = const VerificationMeta('title');
  @override
  late final GeneratedColumn<String> title = GeneratedColumn<String>(
      'title', aliasedName, false,
      type: DriftSqlType.string,
      requiredDuringInsert: false,
      defaultValue: const Constant(''));
  static const VerificationMeta _artistsJsonMeta =
      const VerificationMeta('artistsJson');
  @override
  late final GeneratedColumn<String> artistsJson = GeneratedColumn<String>(
      'artists_json', aliasedName, false,
      type: DriftSqlType.string,
      requiredDuringInsert: false,
      defaultValue: const Constant('[]'));
  static const VerificationMeta _durationMsMeta =
      const VerificationMeta('durationMs');
  @override
  late final GeneratedColumn<int> durationMs = GeneratedColumn<int>(
      'duration_ms', aliasedName, false,
      type: DriftSqlType.int,
      requiredDuringInsert: false,
      defaultValue: const Constant(0));
  static const VerificationMeta _sourceMeta = const VerificationMeta('source');
  @override
  late final GeneratedColumn<String> source = GeneratedColumn<String>(
      'source', aliasedName, false,
      type: DriftSqlType.string,
      requiredDuringInsert: false,
      defaultValue: const Constant(''));
  static const VerificationMeta _streamUrlMeta =
      const VerificationMeta('streamUrl');
  @override
  late final GeneratedColumn<String> streamUrl = GeneratedColumn<String>(
      'stream_url', aliasedName, true,
      type: DriftSqlType.string, requiredDuringInsert: false);
  static const VerificationMeta _streamExpiresAtMeta =
      const VerificationMeta('streamExpiresAt');
  @override
  late final GeneratedColumn<DateTime> streamExpiresAt =
      GeneratedColumn<DateTime>('stream_expires_at', aliasedName, true,
          type: DriftSqlType.dateTime, requiredDuringInsert: false);
  static const VerificationMeta _mimeTypeMeta =
      const VerificationMeta('mimeType');
  @override
  late final GeneratedColumn<String> mimeType = GeneratedColumn<String>(
      'mime_type', aliasedName, true,
      type: DriftSqlType.string, requiredDuringInsert: false);
  static const VerificationMeta _cachedAtMeta =
      const VerificationMeta('cachedAt');
  @override
  late final GeneratedColumn<DateTime> cachedAt = GeneratedColumn<DateTime>(
      'cached_at', aliasedName, false,
      type: DriftSqlType.dateTime,
      requiredDuringInsert: false,
      defaultValue: currentDateAndTime);
  @override
  List<GeneratedColumn> get $columns => [
        queueId,
        position,
        mediaId,
        title,
        artistsJson,
        durationMs,
        source,
        streamUrl,
        streamExpiresAt,
        mimeType,
        cachedAt
      ];
  @override
  String get aliasedName => _alias ?? actualTableName;
  @override
  String get actualTableName => $name;
  static const String $name = 'lookahead_cache';
  @override
  VerificationContext validateIntegrity(Insertable<LookaheadCacheData> instance,
      {bool isInserting = false}) {
    final context = VerificationContext();
    final data = instance.toColumns(true);
    if (data.containsKey('queue_id')) {
      context.handle(_queueIdMeta,
          queueId.isAcceptableOrUnknown(data['queue_id']!, _queueIdMeta));
    } else if (isInserting) {
      context.missing(_queueIdMeta);
    }
    if (data.containsKey('position')) {
      context.handle(_positionMeta,
          position.isAcceptableOrUnknown(data['position']!, _positionMeta));
    } else if (isInserting) {
      context.missing(_positionMeta);
    }
    if (data.containsKey('media_id')) {
      context.handle(_mediaIdMeta,
          mediaId.isAcceptableOrUnknown(data['media_id']!, _mediaIdMeta));
    } else if (isInserting) {
      context.missing(_mediaIdMeta);
    }
    if (data.containsKey('title')) {
      context.handle(
          _titleMeta, title.isAcceptableOrUnknown(data['title']!, _titleMeta));
    }
    if (data.containsKey('artists_json')) {
      context.handle(
          _artistsJsonMeta,
          artistsJson.isAcceptableOrUnknown(
              data['artists_json']!, _artistsJsonMeta));
    }
    if (data.containsKey('duration_ms')) {
      context.handle(
          _durationMsMeta,
          durationMs.isAcceptableOrUnknown(
              data['duration_ms']!, _durationMsMeta));
    }
    if (data.containsKey('source')) {
      context.handle(_sourceMeta,
          source.isAcceptableOrUnknown(data['source']!, _sourceMeta));
    }
    if (data.containsKey('stream_url')) {
      context.handle(_streamUrlMeta,
          streamUrl.isAcceptableOrUnknown(data['stream_url']!, _streamUrlMeta));
    }
    if (data.containsKey('stream_expires_at')) {
      context.handle(
          _streamExpiresAtMeta,
          streamExpiresAt.isAcceptableOrUnknown(
              data['stream_expires_at']!, _streamExpiresAtMeta));
    }
    if (data.containsKey('mime_type')) {
      context.handle(_mimeTypeMeta,
          mimeType.isAcceptableOrUnknown(data['mime_type']!, _mimeTypeMeta));
    }
    if (data.containsKey('cached_at')) {
      context.handle(_cachedAtMeta,
          cachedAt.isAcceptableOrUnknown(data['cached_at']!, _cachedAtMeta));
    }
    return context;
  }

  @override
  Set<GeneratedColumn> get $primaryKey => {queueId, position};
  @override
  LookaheadCacheData map(Map<String, dynamic> data, {String? tablePrefix}) {
    final effectivePrefix = tablePrefix != null ? '$tablePrefix.' : '';
    return LookaheadCacheData(
      queueId: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}queue_id'])!,
      position: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}position'])!,
      mediaId: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}media_id'])!,
      title: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}title'])!,
      artistsJson: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}artists_json'])!,
      durationMs: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}duration_ms'])!,
      source: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}source'])!,
      streamUrl: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}stream_url']),
      streamExpiresAt: attachedDatabase.typeMapping.read(
          DriftSqlType.dateTime, data['${effectivePrefix}stream_expires_at']),
      mimeType: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}mime_type']),
      cachedAt: attachedDatabase.typeMapping
          .read(DriftSqlType.dateTime, data['${effectivePrefix}cached_at'])!,
    );
  }

  @override
  $LookaheadCacheTable createAlias(String alias) {
    return $LookaheadCacheTable(attachedDatabase, alias);
  }
}

class LookaheadCacheData extends DataClass
    implements Insertable<LookaheadCacheData> {
  final String queueId;
  final int position;
  final String mediaId;
  final String title;

  /// Artists serialized as a JSON string array.
  final String artistsJson;
  final int durationMs;
  final String source;
  final String? streamUrl;
  final DateTime? streamExpiresAt;
  final String? mimeType;
  final DateTime cachedAt;
  const LookaheadCacheData(
      {required this.queueId,
      required this.position,
      required this.mediaId,
      required this.title,
      required this.artistsJson,
      required this.durationMs,
      required this.source,
      this.streamUrl,
      this.streamExpiresAt,
      this.mimeType,
      required this.cachedAt});
  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    map['queue_id'] = Variable<String>(queueId);
    map['position'] = Variable<int>(position);
    map['media_id'] = Variable<String>(mediaId);
    map['title'] = Variable<String>(title);
    map['artists_json'] = Variable<String>(artistsJson);
    map['duration_ms'] = Variable<int>(durationMs);
    map['source'] = Variable<String>(source);
    if (!nullToAbsent || streamUrl != null) {
      map['stream_url'] = Variable<String>(streamUrl);
    }
    if (!nullToAbsent || streamExpiresAt != null) {
      map['stream_expires_at'] = Variable<DateTime>(streamExpiresAt);
    }
    if (!nullToAbsent || mimeType != null) {
      map['mime_type'] = Variable<String>(mimeType);
    }
    map['cached_at'] = Variable<DateTime>(cachedAt);
    return map;
  }

  LookaheadCacheCompanion toCompanion(bool nullToAbsent) {
    return LookaheadCacheCompanion(
      queueId: Value(queueId),
      position: Value(position),
      mediaId: Value(mediaId),
      title: Value(title),
      artistsJson: Value(artistsJson),
      durationMs: Value(durationMs),
      source: Value(source),
      streamUrl: streamUrl == null && nullToAbsent
          ? const Value.absent()
          : Value(streamUrl),
      streamExpiresAt: streamExpiresAt == null && nullToAbsent
          ? const Value.absent()
          : Value(streamExpiresAt),
      mimeType: mimeType == null && nullToAbsent
          ? const Value.absent()
          : Value(mimeType),
      cachedAt: Value(cachedAt),
    );
  }

  factory LookaheadCacheData.fromJson(Map<String, dynamic> json,
      {ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return LookaheadCacheData(
      queueId: serializer.fromJson<String>(json['queueId']),
      position: serializer.fromJson<int>(json['position']),
      mediaId: serializer.fromJson<String>(json['mediaId']),
      title: serializer.fromJson<String>(json['title']),
      artistsJson: serializer.fromJson<String>(json['artistsJson']),
      durationMs: serializer.fromJson<int>(json['durationMs']),
      source: serializer.fromJson<String>(json['source']),
      streamUrl: serializer.fromJson<String?>(json['streamUrl']),
      streamExpiresAt: serializer.fromJson<DateTime?>(json['streamExpiresAt']),
      mimeType: serializer.fromJson<String?>(json['mimeType']),
      cachedAt: serializer.fromJson<DateTime>(json['cachedAt']),
    );
  }
  @override
  Map<String, dynamic> toJson({ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return <String, dynamic>{
      'queueId': serializer.toJson<String>(queueId),
      'position': serializer.toJson<int>(position),
      'mediaId': serializer.toJson<String>(mediaId),
      'title': serializer.toJson<String>(title),
      'artistsJson': serializer.toJson<String>(artistsJson),
      'durationMs': serializer.toJson<int>(durationMs),
      'source': serializer.toJson<String>(source),
      'streamUrl': serializer.toJson<String?>(streamUrl),
      'streamExpiresAt': serializer.toJson<DateTime?>(streamExpiresAt),
      'mimeType': serializer.toJson<String?>(mimeType),
      'cachedAt': serializer.toJson<DateTime>(cachedAt),
    };
  }

  LookaheadCacheData copyWith(
          {String? queueId,
          int? position,
          String? mediaId,
          String? title,
          String? artistsJson,
          int? durationMs,
          String? source,
          Value<String?> streamUrl = const Value.absent(),
          Value<DateTime?> streamExpiresAt = const Value.absent(),
          Value<String?> mimeType = const Value.absent(),
          DateTime? cachedAt}) =>
      LookaheadCacheData(
        queueId: queueId ?? this.queueId,
        position: position ?? this.position,
        mediaId: mediaId ?? this.mediaId,
        title: title ?? this.title,
        artistsJson: artistsJson ?? this.artistsJson,
        durationMs: durationMs ?? this.durationMs,
        source: source ?? this.source,
        streamUrl: streamUrl.present ? streamUrl.value : this.streamUrl,
        streamExpiresAt: streamExpiresAt.present
            ? streamExpiresAt.value
            : this.streamExpiresAt,
        mimeType: mimeType.present ? mimeType.value : this.mimeType,
        cachedAt: cachedAt ?? this.cachedAt,
      );
  LookaheadCacheData copyWithCompanion(LookaheadCacheCompanion data) {
    return LookaheadCacheData(
      queueId: data.queueId.present ? data.queueId.value : this.queueId,
      position: data.position.present ? data.position.value : this.position,
      mediaId: data.mediaId.present ? data.mediaId.value : this.mediaId,
      title: data.title.present ? data.title.value : this.title,
      artistsJson:
          data.artistsJson.present ? data.artistsJson.value : this.artistsJson,
      durationMs:
          data.durationMs.present ? data.durationMs.value : this.durationMs,
      source: data.source.present ? data.source.value : this.source,
      streamUrl: data.streamUrl.present ? data.streamUrl.value : this.streamUrl,
      streamExpiresAt: data.streamExpiresAt.present
          ? data.streamExpiresAt.value
          : this.streamExpiresAt,
      mimeType: data.mimeType.present ? data.mimeType.value : this.mimeType,
      cachedAt: data.cachedAt.present ? data.cachedAt.value : this.cachedAt,
    );
  }

  @override
  String toString() {
    return (StringBuffer('LookaheadCacheData(')
          ..write('queueId: $queueId, ')
          ..write('position: $position, ')
          ..write('mediaId: $mediaId, ')
          ..write('title: $title, ')
          ..write('artistsJson: $artistsJson, ')
          ..write('durationMs: $durationMs, ')
          ..write('source: $source, ')
          ..write('streamUrl: $streamUrl, ')
          ..write('streamExpiresAt: $streamExpiresAt, ')
          ..write('mimeType: $mimeType, ')
          ..write('cachedAt: $cachedAt')
          ..write(')'))
        .toString();
  }

  @override
  int get hashCode => Object.hash(
      queueId,
      position,
      mediaId,
      title,
      artistsJson,
      durationMs,
      source,
      streamUrl,
      streamExpiresAt,
      mimeType,
      cachedAt);
  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is LookaheadCacheData &&
          other.queueId == this.queueId &&
          other.position == this.position &&
          other.mediaId == this.mediaId &&
          other.title == this.title &&
          other.artistsJson == this.artistsJson &&
          other.durationMs == this.durationMs &&
          other.source == this.source &&
          other.streamUrl == this.streamUrl &&
          other.streamExpiresAt == this.streamExpiresAt &&
          other.mimeType == this.mimeType &&
          other.cachedAt == this.cachedAt);
}

class LookaheadCacheCompanion extends UpdateCompanion<LookaheadCacheData> {
  final Value<String> queueId;
  final Value<int> position;
  final Value<String> mediaId;
  final Value<String> title;
  final Value<String> artistsJson;
  final Value<int> durationMs;
  final Value<String> source;
  final Value<String?> streamUrl;
  final Value<DateTime?> streamExpiresAt;
  final Value<String?> mimeType;
  final Value<DateTime> cachedAt;
  final Value<int> rowid;
  const LookaheadCacheCompanion({
    this.queueId = const Value.absent(),
    this.position = const Value.absent(),
    this.mediaId = const Value.absent(),
    this.title = const Value.absent(),
    this.artistsJson = const Value.absent(),
    this.durationMs = const Value.absent(),
    this.source = const Value.absent(),
    this.streamUrl = const Value.absent(),
    this.streamExpiresAt = const Value.absent(),
    this.mimeType = const Value.absent(),
    this.cachedAt = const Value.absent(),
    this.rowid = const Value.absent(),
  });
  LookaheadCacheCompanion.insert({
    required String queueId,
    required int position,
    required String mediaId,
    this.title = const Value.absent(),
    this.artistsJson = const Value.absent(),
    this.durationMs = const Value.absent(),
    this.source = const Value.absent(),
    this.streamUrl = const Value.absent(),
    this.streamExpiresAt = const Value.absent(),
    this.mimeType = const Value.absent(),
    this.cachedAt = const Value.absent(),
    this.rowid = const Value.absent(),
  })  : queueId = Value(queueId),
        position = Value(position),
        mediaId = Value(mediaId);
  static Insertable<LookaheadCacheData> custom({
    Expression<String>? queueId,
    Expression<int>? position,
    Expression<String>? mediaId,
    Expression<String>? title,
    Expression<String>? artistsJson,
    Expression<int>? durationMs,
    Expression<String>? source,
    Expression<String>? streamUrl,
    Expression<DateTime>? streamExpiresAt,
    Expression<String>? mimeType,
    Expression<DateTime>? cachedAt,
    Expression<int>? rowid,
  }) {
    return RawValuesInsertable({
      if (queueId != null) 'queue_id': queueId,
      if (position != null) 'position': position,
      if (mediaId != null) 'media_id': mediaId,
      if (title != null) 'title': title,
      if (artistsJson != null) 'artists_json': artistsJson,
      if (durationMs != null) 'duration_ms': durationMs,
      if (source != null) 'source': source,
      if (streamUrl != null) 'stream_url': streamUrl,
      if (streamExpiresAt != null) 'stream_expires_at': streamExpiresAt,
      if (mimeType != null) 'mime_type': mimeType,
      if (cachedAt != null) 'cached_at': cachedAt,
      if (rowid != null) 'rowid': rowid,
    });
  }

  LookaheadCacheCompanion copyWith(
      {Value<String>? queueId,
      Value<int>? position,
      Value<String>? mediaId,
      Value<String>? title,
      Value<String>? artistsJson,
      Value<int>? durationMs,
      Value<String>? source,
      Value<String?>? streamUrl,
      Value<DateTime?>? streamExpiresAt,
      Value<String?>? mimeType,
      Value<DateTime>? cachedAt,
      Value<int>? rowid}) {
    return LookaheadCacheCompanion(
      queueId: queueId ?? this.queueId,
      position: position ?? this.position,
      mediaId: mediaId ?? this.mediaId,
      title: title ?? this.title,
      artistsJson: artistsJson ?? this.artistsJson,
      durationMs: durationMs ?? this.durationMs,
      source: source ?? this.source,
      streamUrl: streamUrl ?? this.streamUrl,
      streamExpiresAt: streamExpiresAt ?? this.streamExpiresAt,
      mimeType: mimeType ?? this.mimeType,
      cachedAt: cachedAt ?? this.cachedAt,
      rowid: rowid ?? this.rowid,
    );
  }

  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    if (queueId.present) {
      map['queue_id'] = Variable<String>(queueId.value);
    }
    if (position.present) {
      map['position'] = Variable<int>(position.value);
    }
    if (mediaId.present) {
      map['media_id'] = Variable<String>(mediaId.value);
    }
    if (title.present) {
      map['title'] = Variable<String>(title.value);
    }
    if (artistsJson.present) {
      map['artists_json'] = Variable<String>(artistsJson.value);
    }
    if (durationMs.present) {
      map['duration_ms'] = Variable<int>(durationMs.value);
    }
    if (source.present) {
      map['source'] = Variable<String>(source.value);
    }
    if (streamUrl.present) {
      map['stream_url'] = Variable<String>(streamUrl.value);
    }
    if (streamExpiresAt.present) {
      map['stream_expires_at'] = Variable<DateTime>(streamExpiresAt.value);
    }
    if (mimeType.present) {
      map['mime_type'] = Variable<String>(mimeType.value);
    }
    if (cachedAt.present) {
      map['cached_at'] = Variable<DateTime>(cachedAt.value);
    }
    if (rowid.present) {
      map['rowid'] = Variable<int>(rowid.value);
    }
    return map;
  }

  @override
  String toString() {
    return (StringBuffer('LookaheadCacheCompanion(')
          ..write('queueId: $queueId, ')
          ..write('position: $position, ')
          ..write('mediaId: $mediaId, ')
          ..write('title: $title, ')
          ..write('artistsJson: $artistsJson, ')
          ..write('durationMs: $durationMs, ')
          ..write('source: $source, ')
          ..write('streamUrl: $streamUrl, ')
          ..write('streamExpiresAt: $streamExpiresAt, ')
          ..write('mimeType: $mimeType, ')
          ..write('cachedAt: $cachedAt, ')
          ..write('rowid: $rowid')
          ..write(')'))
        .toString();
  }
}

class $RecentPlaysTable extends RecentPlays
    with TableInfo<$RecentPlaysTable, RecentPlay> {
  @override
  final GeneratedDatabase attachedDatabase;
  final String? _alias;
  $RecentPlaysTable(this.attachedDatabase, [this._alias]);
  static const VerificationMeta _mediaIdMeta =
      const VerificationMeta('mediaId');
  @override
  late final GeneratedColumn<String> mediaId = GeneratedColumn<String>(
      'media_id', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _titleMeta = const VerificationMeta('title');
  @override
  late final GeneratedColumn<String> title = GeneratedColumn<String>(
      'title', aliasedName, false,
      type: DriftSqlType.string,
      requiredDuringInsert: false,
      defaultValue: const Constant(''));
  static const VerificationMeta _artistNameMeta =
      const VerificationMeta('artistName');
  @override
  late final GeneratedColumn<String> artistName = GeneratedColumn<String>(
      'artist_name', aliasedName, false,
      type: DriftSqlType.string,
      requiredDuringInsert: false,
      defaultValue: const Constant(''));
  static const VerificationMeta _sourceMeta = const VerificationMeta('source');
  @override
  late final GeneratedColumn<String> source = GeneratedColumn<String>(
      'source', aliasedName, false,
      type: DriftSqlType.string,
      requiredDuringInsert: false,
      defaultValue: const Constant(''));
  static const VerificationMeta _streamUrlMeta =
      const VerificationMeta('streamUrl');
  @override
  late final GeneratedColumn<String> streamUrl = GeneratedColumn<String>(
      'stream_url', aliasedName, true,
      type: DriftSqlType.string, requiredDuringInsert: false);
  static const VerificationMeta _durationMsMeta =
      const VerificationMeta('durationMs');
  @override
  late final GeneratedColumn<int> durationMs = GeneratedColumn<int>(
      'duration_ms', aliasedName, false,
      type: DriftSqlType.int,
      requiredDuringInsert: false,
      defaultValue: const Constant(0));
  static const VerificationMeta _playCountMeta =
      const VerificationMeta('playCount');
  @override
  late final GeneratedColumn<int> playCount = GeneratedColumn<int>(
      'play_count', aliasedName, false,
      type: DriftSqlType.int,
      requiredDuringInsert: false,
      defaultValue: const Constant(1));
  static const VerificationMeta _lastPlayedAtMeta =
      const VerificationMeta('lastPlayedAt');
  @override
  late final GeneratedColumn<DateTime> lastPlayedAt = GeneratedColumn<DateTime>(
      'last_played_at', aliasedName, false,
      type: DriftSqlType.dateTime,
      requiredDuringInsert: false,
      defaultValue: currentDateAndTime);
  @override
  List<GeneratedColumn> get $columns => [
        mediaId,
        title,
        artistName,
        source,
        streamUrl,
        durationMs,
        playCount,
        lastPlayedAt
      ];
  @override
  String get aliasedName => _alias ?? actualTableName;
  @override
  String get actualTableName => $name;
  static const String $name = 'recent_plays';
  @override
  VerificationContext validateIntegrity(Insertable<RecentPlay> instance,
      {bool isInserting = false}) {
    final context = VerificationContext();
    final data = instance.toColumns(true);
    if (data.containsKey('media_id')) {
      context.handle(_mediaIdMeta,
          mediaId.isAcceptableOrUnknown(data['media_id']!, _mediaIdMeta));
    } else if (isInserting) {
      context.missing(_mediaIdMeta);
    }
    if (data.containsKey('title')) {
      context.handle(
          _titleMeta, title.isAcceptableOrUnknown(data['title']!, _titleMeta));
    }
    if (data.containsKey('artist_name')) {
      context.handle(
          _artistNameMeta,
          artistName.isAcceptableOrUnknown(
              data['artist_name']!, _artistNameMeta));
    }
    if (data.containsKey('source')) {
      context.handle(_sourceMeta,
          source.isAcceptableOrUnknown(data['source']!, _sourceMeta));
    }
    if (data.containsKey('stream_url')) {
      context.handle(_streamUrlMeta,
          streamUrl.isAcceptableOrUnknown(data['stream_url']!, _streamUrlMeta));
    }
    if (data.containsKey('duration_ms')) {
      context.handle(
          _durationMsMeta,
          durationMs.isAcceptableOrUnknown(
              data['duration_ms']!, _durationMsMeta));
    }
    if (data.containsKey('play_count')) {
      context.handle(_playCountMeta,
          playCount.isAcceptableOrUnknown(data['play_count']!, _playCountMeta));
    }
    if (data.containsKey('last_played_at')) {
      context.handle(
          _lastPlayedAtMeta,
          lastPlayedAt.isAcceptableOrUnknown(
              data['last_played_at']!, _lastPlayedAtMeta));
    }
    return context;
  }

  @override
  Set<GeneratedColumn> get $primaryKey => {mediaId};
  @override
  RecentPlay map(Map<String, dynamic> data, {String? tablePrefix}) {
    final effectivePrefix = tablePrefix != null ? '$tablePrefix.' : '';
    return RecentPlay(
      mediaId: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}media_id'])!,
      title: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}title'])!,
      artistName: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}artist_name'])!,
      source: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}source'])!,
      streamUrl: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}stream_url']),
      durationMs: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}duration_ms'])!,
      playCount: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}play_count'])!,
      lastPlayedAt: attachedDatabase.typeMapping.read(
          DriftSqlType.dateTime, data['${effectivePrefix}last_played_at'])!,
    );
  }

  @override
  $RecentPlaysTable createAlias(String alias) {
    return $RecentPlaysTable(attachedDatabase, alias);
  }
}

class RecentPlay extends DataClass implements Insertable<RecentPlay> {
  final String mediaId;
  final String title;
  final String artistName;
  final String source;
  final String? streamUrl;
  final int durationMs;
  final int playCount;
  final DateTime lastPlayedAt;
  const RecentPlay(
      {required this.mediaId,
      required this.title,
      required this.artistName,
      required this.source,
      this.streamUrl,
      required this.durationMs,
      required this.playCount,
      required this.lastPlayedAt});
  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    map['media_id'] = Variable<String>(mediaId);
    map['title'] = Variable<String>(title);
    map['artist_name'] = Variable<String>(artistName);
    map['source'] = Variable<String>(source);
    if (!nullToAbsent || streamUrl != null) {
      map['stream_url'] = Variable<String>(streamUrl);
    }
    map['duration_ms'] = Variable<int>(durationMs);
    map['play_count'] = Variable<int>(playCount);
    map['last_played_at'] = Variable<DateTime>(lastPlayedAt);
    return map;
  }

  RecentPlaysCompanion toCompanion(bool nullToAbsent) {
    return RecentPlaysCompanion(
      mediaId: Value(mediaId),
      title: Value(title),
      artistName: Value(artistName),
      source: Value(source),
      streamUrl: streamUrl == null && nullToAbsent
          ? const Value.absent()
          : Value(streamUrl),
      durationMs: Value(durationMs),
      playCount: Value(playCount),
      lastPlayedAt: Value(lastPlayedAt),
    );
  }

  factory RecentPlay.fromJson(Map<String, dynamic> json,
      {ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return RecentPlay(
      mediaId: serializer.fromJson<String>(json['mediaId']),
      title: serializer.fromJson<String>(json['title']),
      artistName: serializer.fromJson<String>(json['artistName']),
      source: serializer.fromJson<String>(json['source']),
      streamUrl: serializer.fromJson<String?>(json['streamUrl']),
      durationMs: serializer.fromJson<int>(json['durationMs']),
      playCount: serializer.fromJson<int>(json['playCount']),
      lastPlayedAt: serializer.fromJson<DateTime>(json['lastPlayedAt']),
    );
  }
  @override
  Map<String, dynamic> toJson({ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return <String, dynamic>{
      'mediaId': serializer.toJson<String>(mediaId),
      'title': serializer.toJson<String>(title),
      'artistName': serializer.toJson<String>(artistName),
      'source': serializer.toJson<String>(source),
      'streamUrl': serializer.toJson<String?>(streamUrl),
      'durationMs': serializer.toJson<int>(durationMs),
      'playCount': serializer.toJson<int>(playCount),
      'lastPlayedAt': serializer.toJson<DateTime>(lastPlayedAt),
    };
  }

  RecentPlay copyWith(
          {String? mediaId,
          String? title,
          String? artistName,
          String? source,
          Value<String?> streamUrl = const Value.absent(),
          int? durationMs,
          int? playCount,
          DateTime? lastPlayedAt}) =>
      RecentPlay(
        mediaId: mediaId ?? this.mediaId,
        title: title ?? this.title,
        artistName: artistName ?? this.artistName,
        source: source ?? this.source,
        streamUrl: streamUrl.present ? streamUrl.value : this.streamUrl,
        durationMs: durationMs ?? this.durationMs,
        playCount: playCount ?? this.playCount,
        lastPlayedAt: lastPlayedAt ?? this.lastPlayedAt,
      );
  RecentPlay copyWithCompanion(RecentPlaysCompanion data) {
    return RecentPlay(
      mediaId: data.mediaId.present ? data.mediaId.value : this.mediaId,
      title: data.title.present ? data.title.value : this.title,
      artistName:
          data.artistName.present ? data.artistName.value : this.artistName,
      source: data.source.present ? data.source.value : this.source,
      streamUrl: data.streamUrl.present ? data.streamUrl.value : this.streamUrl,
      durationMs:
          data.durationMs.present ? data.durationMs.value : this.durationMs,
      playCount: data.playCount.present ? data.playCount.value : this.playCount,
      lastPlayedAt: data.lastPlayedAt.present
          ? data.lastPlayedAt.value
          : this.lastPlayedAt,
    );
  }

  @override
  String toString() {
    return (StringBuffer('RecentPlay(')
          ..write('mediaId: $mediaId, ')
          ..write('title: $title, ')
          ..write('artistName: $artistName, ')
          ..write('source: $source, ')
          ..write('streamUrl: $streamUrl, ')
          ..write('durationMs: $durationMs, ')
          ..write('playCount: $playCount, ')
          ..write('lastPlayedAt: $lastPlayedAt')
          ..write(')'))
        .toString();
  }

  @override
  int get hashCode => Object.hash(mediaId, title, artistName, source, streamUrl,
      durationMs, playCount, lastPlayedAt);
  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is RecentPlay &&
          other.mediaId == this.mediaId &&
          other.title == this.title &&
          other.artistName == this.artistName &&
          other.source == this.source &&
          other.streamUrl == this.streamUrl &&
          other.durationMs == this.durationMs &&
          other.playCount == this.playCount &&
          other.lastPlayedAt == this.lastPlayedAt);
}

class RecentPlaysCompanion extends UpdateCompanion<RecentPlay> {
  final Value<String> mediaId;
  final Value<String> title;
  final Value<String> artistName;
  final Value<String> source;
  final Value<String?> streamUrl;
  final Value<int> durationMs;
  final Value<int> playCount;
  final Value<DateTime> lastPlayedAt;
  final Value<int> rowid;
  const RecentPlaysCompanion({
    this.mediaId = const Value.absent(),
    this.title = const Value.absent(),
    this.artistName = const Value.absent(),
    this.source = const Value.absent(),
    this.streamUrl = const Value.absent(),
    this.durationMs = const Value.absent(),
    this.playCount = const Value.absent(),
    this.lastPlayedAt = const Value.absent(),
    this.rowid = const Value.absent(),
  });
  RecentPlaysCompanion.insert({
    required String mediaId,
    this.title = const Value.absent(),
    this.artistName = const Value.absent(),
    this.source = const Value.absent(),
    this.streamUrl = const Value.absent(),
    this.durationMs = const Value.absent(),
    this.playCount = const Value.absent(),
    this.lastPlayedAt = const Value.absent(),
    this.rowid = const Value.absent(),
  }) : mediaId = Value(mediaId);
  static Insertable<RecentPlay> custom({
    Expression<String>? mediaId,
    Expression<String>? title,
    Expression<String>? artistName,
    Expression<String>? source,
    Expression<String>? streamUrl,
    Expression<int>? durationMs,
    Expression<int>? playCount,
    Expression<DateTime>? lastPlayedAt,
    Expression<int>? rowid,
  }) {
    return RawValuesInsertable({
      if (mediaId != null) 'media_id': mediaId,
      if (title != null) 'title': title,
      if (artistName != null) 'artist_name': artistName,
      if (source != null) 'source': source,
      if (streamUrl != null) 'stream_url': streamUrl,
      if (durationMs != null) 'duration_ms': durationMs,
      if (playCount != null) 'play_count': playCount,
      if (lastPlayedAt != null) 'last_played_at': lastPlayedAt,
      if (rowid != null) 'rowid': rowid,
    });
  }

  RecentPlaysCompanion copyWith(
      {Value<String>? mediaId,
      Value<String>? title,
      Value<String>? artistName,
      Value<String>? source,
      Value<String?>? streamUrl,
      Value<int>? durationMs,
      Value<int>? playCount,
      Value<DateTime>? lastPlayedAt,
      Value<int>? rowid}) {
    return RecentPlaysCompanion(
      mediaId: mediaId ?? this.mediaId,
      title: title ?? this.title,
      artistName: artistName ?? this.artistName,
      source: source ?? this.source,
      streamUrl: streamUrl ?? this.streamUrl,
      durationMs: durationMs ?? this.durationMs,
      playCount: playCount ?? this.playCount,
      lastPlayedAt: lastPlayedAt ?? this.lastPlayedAt,
      rowid: rowid ?? this.rowid,
    );
  }

  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    if (mediaId.present) {
      map['media_id'] = Variable<String>(mediaId.value);
    }
    if (title.present) {
      map['title'] = Variable<String>(title.value);
    }
    if (artistName.present) {
      map['artist_name'] = Variable<String>(artistName.value);
    }
    if (source.present) {
      map['source'] = Variable<String>(source.value);
    }
    if (streamUrl.present) {
      map['stream_url'] = Variable<String>(streamUrl.value);
    }
    if (durationMs.present) {
      map['duration_ms'] = Variable<int>(durationMs.value);
    }
    if (playCount.present) {
      map['play_count'] = Variable<int>(playCount.value);
    }
    if (lastPlayedAt.present) {
      map['last_played_at'] = Variable<DateTime>(lastPlayedAt.value);
    }
    if (rowid.present) {
      map['rowid'] = Variable<int>(rowid.value);
    }
    return map;
  }

  @override
  String toString() {
    return (StringBuffer('RecentPlaysCompanion(')
          ..write('mediaId: $mediaId, ')
          ..write('title: $title, ')
          ..write('artistName: $artistName, ')
          ..write('source: $source, ')
          ..write('streamUrl: $streamUrl, ')
          ..write('durationMs: $durationMs, ')
          ..write('playCount: $playCount, ')
          ..write('lastPlayedAt: $lastPlayedAt, ')
          ..write('rowid: $rowid')
          ..write(')'))
        .toString();
  }
}

class $HomeCacheTable extends HomeCache
    with TableInfo<$HomeCacheTable, HomeCacheData> {
  @override
  final GeneratedDatabase attachedDatabase;
  final String? _alias;
  $HomeCacheTable(this.attachedDatabase, [this._alias]);
  static const VerificationMeta _cacheKeyMeta =
      const VerificationMeta('cacheKey');
  @override
  late final GeneratedColumn<String> cacheKey = GeneratedColumn<String>(
      'cache_key', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _payloadJsonMeta =
      const VerificationMeta('payloadJson');
  @override
  late final GeneratedColumn<String> payloadJson = GeneratedColumn<String>(
      'payload_json', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _cachedAtMeta =
      const VerificationMeta('cachedAt');
  @override
  late final GeneratedColumn<DateTime> cachedAt = GeneratedColumn<DateTime>(
      'cached_at', aliasedName, false,
      type: DriftSqlType.dateTime,
      requiredDuringInsert: false,
      defaultValue: currentDateAndTime);
  @override
  List<GeneratedColumn> get $columns => [cacheKey, payloadJson, cachedAt];
  @override
  String get aliasedName => _alias ?? actualTableName;
  @override
  String get actualTableName => $name;
  static const String $name = 'home_cache';
  @override
  VerificationContext validateIntegrity(Insertable<HomeCacheData> instance,
      {bool isInserting = false}) {
    final context = VerificationContext();
    final data = instance.toColumns(true);
    if (data.containsKey('cache_key')) {
      context.handle(_cacheKeyMeta,
          cacheKey.isAcceptableOrUnknown(data['cache_key']!, _cacheKeyMeta));
    } else if (isInserting) {
      context.missing(_cacheKeyMeta);
    }
    if (data.containsKey('payload_json')) {
      context.handle(
          _payloadJsonMeta,
          payloadJson.isAcceptableOrUnknown(
              data['payload_json']!, _payloadJsonMeta));
    } else if (isInserting) {
      context.missing(_payloadJsonMeta);
    }
    if (data.containsKey('cached_at')) {
      context.handle(_cachedAtMeta,
          cachedAt.isAcceptableOrUnknown(data['cached_at']!, _cachedAtMeta));
    }
    return context;
  }

  @override
  Set<GeneratedColumn> get $primaryKey => {cacheKey};
  @override
  HomeCacheData map(Map<String, dynamic> data, {String? tablePrefix}) {
    final effectivePrefix = tablePrefix != null ? '$tablePrefix.' : '';
    return HomeCacheData(
      cacheKey: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}cache_key'])!,
      payloadJson: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}payload_json'])!,
      cachedAt: attachedDatabase.typeMapping
          .read(DriftSqlType.dateTime, data['${effectivePrefix}cached_at'])!,
    );
  }

  @override
  $HomeCacheTable createAlias(String alias) {
    return $HomeCacheTable(attachedDatabase, alias);
  }
}

class HomeCacheData extends DataClass implements Insertable<HomeCacheData> {
  final String cacheKey;

  /// The full `/home` response JSON (sections + chips), stored verbatim.
  final String payloadJson;
  final DateTime cachedAt;
  const HomeCacheData(
      {required this.cacheKey,
      required this.payloadJson,
      required this.cachedAt});
  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    map['cache_key'] = Variable<String>(cacheKey);
    map['payload_json'] = Variable<String>(payloadJson);
    map['cached_at'] = Variable<DateTime>(cachedAt);
    return map;
  }

  HomeCacheCompanion toCompanion(bool nullToAbsent) {
    return HomeCacheCompanion(
      cacheKey: Value(cacheKey),
      payloadJson: Value(payloadJson),
      cachedAt: Value(cachedAt),
    );
  }

  factory HomeCacheData.fromJson(Map<String, dynamic> json,
      {ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return HomeCacheData(
      cacheKey: serializer.fromJson<String>(json['cacheKey']),
      payloadJson: serializer.fromJson<String>(json['payloadJson']),
      cachedAt: serializer.fromJson<DateTime>(json['cachedAt']),
    );
  }
  @override
  Map<String, dynamic> toJson({ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return <String, dynamic>{
      'cacheKey': serializer.toJson<String>(cacheKey),
      'payloadJson': serializer.toJson<String>(payloadJson),
      'cachedAt': serializer.toJson<DateTime>(cachedAt),
    };
  }

  HomeCacheData copyWith(
          {String? cacheKey, String? payloadJson, DateTime? cachedAt}) =>
      HomeCacheData(
        cacheKey: cacheKey ?? this.cacheKey,
        payloadJson: payloadJson ?? this.payloadJson,
        cachedAt: cachedAt ?? this.cachedAt,
      );
  HomeCacheData copyWithCompanion(HomeCacheCompanion data) {
    return HomeCacheData(
      cacheKey: data.cacheKey.present ? data.cacheKey.value : this.cacheKey,
      payloadJson:
          data.payloadJson.present ? data.payloadJson.value : this.payloadJson,
      cachedAt: data.cachedAt.present ? data.cachedAt.value : this.cachedAt,
    );
  }

  @override
  String toString() {
    return (StringBuffer('HomeCacheData(')
          ..write('cacheKey: $cacheKey, ')
          ..write('payloadJson: $payloadJson, ')
          ..write('cachedAt: $cachedAt')
          ..write(')'))
        .toString();
  }

  @override
  int get hashCode => Object.hash(cacheKey, payloadJson, cachedAt);
  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is HomeCacheData &&
          other.cacheKey == this.cacheKey &&
          other.payloadJson == this.payloadJson &&
          other.cachedAt == this.cachedAt);
}

class HomeCacheCompanion extends UpdateCompanion<HomeCacheData> {
  final Value<String> cacheKey;
  final Value<String> payloadJson;
  final Value<DateTime> cachedAt;
  final Value<int> rowid;
  const HomeCacheCompanion({
    this.cacheKey = const Value.absent(),
    this.payloadJson = const Value.absent(),
    this.cachedAt = const Value.absent(),
    this.rowid = const Value.absent(),
  });
  HomeCacheCompanion.insert({
    required String cacheKey,
    required String payloadJson,
    this.cachedAt = const Value.absent(),
    this.rowid = const Value.absent(),
  })  : cacheKey = Value(cacheKey),
        payloadJson = Value(payloadJson);
  static Insertable<HomeCacheData> custom({
    Expression<String>? cacheKey,
    Expression<String>? payloadJson,
    Expression<DateTime>? cachedAt,
    Expression<int>? rowid,
  }) {
    return RawValuesInsertable({
      if (cacheKey != null) 'cache_key': cacheKey,
      if (payloadJson != null) 'payload_json': payloadJson,
      if (cachedAt != null) 'cached_at': cachedAt,
      if (rowid != null) 'rowid': rowid,
    });
  }

  HomeCacheCompanion copyWith(
      {Value<String>? cacheKey,
      Value<String>? payloadJson,
      Value<DateTime>? cachedAt,
      Value<int>? rowid}) {
    return HomeCacheCompanion(
      cacheKey: cacheKey ?? this.cacheKey,
      payloadJson: payloadJson ?? this.payloadJson,
      cachedAt: cachedAt ?? this.cachedAt,
      rowid: rowid ?? this.rowid,
    );
  }

  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    if (cacheKey.present) {
      map['cache_key'] = Variable<String>(cacheKey.value);
    }
    if (payloadJson.present) {
      map['payload_json'] = Variable<String>(payloadJson.value);
    }
    if (cachedAt.present) {
      map['cached_at'] = Variable<DateTime>(cachedAt.value);
    }
    if (rowid.present) {
      map['rowid'] = Variable<int>(rowid.value);
    }
    return map;
  }

  @override
  String toString() {
    return (StringBuffer('HomeCacheCompanion(')
          ..write('cacheKey: $cacheKey, ')
          ..write('payloadJson: $payloadJson, ')
          ..write('cachedAt: $cachedAt, ')
          ..write('rowid: $rowid')
          ..write(')'))
        .toString();
  }
}

class $DownloadJobsTable extends DownloadJobs
    with TableInfo<$DownloadJobsTable, DownloadJob> {
  @override
  final GeneratedDatabase attachedDatabase;
  final String? _alias;
  $DownloadJobsTable(this.attachedDatabase, [this._alias]);
  static const VerificationMeta _mediaIdMeta =
      const VerificationMeta('mediaId');
  @override
  late final GeneratedColumn<String> mediaId = GeneratedColumn<String>(
      'media_id', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _titleMeta = const VerificationMeta('title');
  @override
  late final GeneratedColumn<String> title = GeneratedColumn<String>(
      'title', aliasedName, false,
      type: DriftSqlType.string,
      requiredDuringInsert: false,
      defaultValue: const Constant(''));
  static const VerificationMeta _sourceUrlMeta =
      const VerificationMeta('sourceUrl');
  @override
  late final GeneratedColumn<String> sourceUrl = GeneratedColumn<String>(
      'source_url', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _statusMeta = const VerificationMeta('status');
  @override
  late final GeneratedColumn<String> status = GeneratedColumn<String>(
      'status', aliasedName, false,
      type: DriftSqlType.string,
      requiredDuringInsert: false,
      defaultValue: const Constant('pending'));
  static const VerificationMeta _totalBytesMeta =
      const VerificationMeta('totalBytes');
  @override
  late final GeneratedColumn<int> totalBytes = GeneratedColumn<int>(
      'total_bytes', aliasedName, false,
      type: DriftSqlType.int,
      requiredDuringInsert: false,
      defaultValue: const Constant(0));
  static const VerificationMeta _receivedBytesMeta =
      const VerificationMeta('receivedBytes');
  @override
  late final GeneratedColumn<int> receivedBytes = GeneratedColumn<int>(
      'received_bytes', aliasedName, false,
      type: DriftSqlType.int,
      requiredDuringInsert: false,
      defaultValue: const Constant(0));
  static const VerificationMeta _playlistIdMeta =
      const VerificationMeta('playlistId');
  @override
  late final GeneratedColumn<String> playlistId = GeneratedColumn<String>(
      'playlist_id', aliasedName, true,
      type: DriftSqlType.string, requiredDuringInsert: false);
  static const VerificationMeta _errorMeta = const VerificationMeta('error');
  @override
  late final GeneratedColumn<String> error = GeneratedColumn<String>(
      'error', aliasedName, true,
      type: DriftSqlType.string, requiredDuringInsert: false);
  static const VerificationMeta _updatedAtMeta =
      const VerificationMeta('updatedAt');
  @override
  late final GeneratedColumn<DateTime> updatedAt = GeneratedColumn<DateTime>(
      'updated_at', aliasedName, false,
      type: DriftSqlType.dateTime,
      requiredDuringInsert: false,
      defaultValue: currentDateAndTime);
  @override
  List<GeneratedColumn> get $columns => [
        mediaId,
        title,
        sourceUrl,
        status,
        totalBytes,
        receivedBytes,
        playlistId,
        error,
        updatedAt
      ];
  @override
  String get aliasedName => _alias ?? actualTableName;
  @override
  String get actualTableName => $name;
  static const String $name = 'download_jobs';
  @override
  VerificationContext validateIntegrity(Insertable<DownloadJob> instance,
      {bool isInserting = false}) {
    final context = VerificationContext();
    final data = instance.toColumns(true);
    if (data.containsKey('media_id')) {
      context.handle(_mediaIdMeta,
          mediaId.isAcceptableOrUnknown(data['media_id']!, _mediaIdMeta));
    } else if (isInserting) {
      context.missing(_mediaIdMeta);
    }
    if (data.containsKey('title')) {
      context.handle(
          _titleMeta, title.isAcceptableOrUnknown(data['title']!, _titleMeta));
    }
    if (data.containsKey('source_url')) {
      context.handle(_sourceUrlMeta,
          sourceUrl.isAcceptableOrUnknown(data['source_url']!, _sourceUrlMeta));
    } else if (isInserting) {
      context.missing(_sourceUrlMeta);
    }
    if (data.containsKey('status')) {
      context.handle(_statusMeta,
          status.isAcceptableOrUnknown(data['status']!, _statusMeta));
    }
    if (data.containsKey('total_bytes')) {
      context.handle(
          _totalBytesMeta,
          totalBytes.isAcceptableOrUnknown(
              data['total_bytes']!, _totalBytesMeta));
    }
    if (data.containsKey('received_bytes')) {
      context.handle(
          _receivedBytesMeta,
          receivedBytes.isAcceptableOrUnknown(
              data['received_bytes']!, _receivedBytesMeta));
    }
    if (data.containsKey('playlist_id')) {
      context.handle(
          _playlistIdMeta,
          playlistId.isAcceptableOrUnknown(
              data['playlist_id']!, _playlistIdMeta));
    }
    if (data.containsKey('error')) {
      context.handle(
          _errorMeta, error.isAcceptableOrUnknown(data['error']!, _errorMeta));
    }
    if (data.containsKey('updated_at')) {
      context.handle(_updatedAtMeta,
          updatedAt.isAcceptableOrUnknown(data['updated_at']!, _updatedAtMeta));
    }
    return context;
  }

  @override
  Set<GeneratedColumn> get $primaryKey => {mediaId};
  @override
  DownloadJob map(Map<String, dynamic> data, {String? tablePrefix}) {
    final effectivePrefix = tablePrefix != null ? '$tablePrefix.' : '';
    return DownloadJob(
      mediaId: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}media_id'])!,
      title: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}title'])!,
      sourceUrl: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}source_url'])!,
      status: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}status'])!,
      totalBytes: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}total_bytes'])!,
      receivedBytes: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}received_bytes'])!,
      playlistId: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}playlist_id']),
      error: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}error']),
      updatedAt: attachedDatabase.typeMapping
          .read(DriftSqlType.dateTime, data['${effectivePrefix}updated_at'])!,
    );
  }

  @override
  $DownloadJobsTable createAlias(String alias) {
    return $DownloadJobsTable(attachedDatabase, alias);
  }
}

class DownloadJob extends DataClass implements Insertable<DownloadJob> {
  final String mediaId;
  final String title;

  /// The remote URL to fetch (server stream URL for local songs; resolved YT
  /// URL for remote, best-effort).
  final String sourceUrl;
  final String status;
  final int totalBytes;
  final int receivedBytes;

  /// Optional playlist this job was enqueued for (per-playlist downloads).
  final String? playlistId;
  final String? error;
  final DateTime updatedAt;
  const DownloadJob(
      {required this.mediaId,
      required this.title,
      required this.sourceUrl,
      required this.status,
      required this.totalBytes,
      required this.receivedBytes,
      this.playlistId,
      this.error,
      required this.updatedAt});
  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    map['media_id'] = Variable<String>(mediaId);
    map['title'] = Variable<String>(title);
    map['source_url'] = Variable<String>(sourceUrl);
    map['status'] = Variable<String>(status);
    map['total_bytes'] = Variable<int>(totalBytes);
    map['received_bytes'] = Variable<int>(receivedBytes);
    if (!nullToAbsent || playlistId != null) {
      map['playlist_id'] = Variable<String>(playlistId);
    }
    if (!nullToAbsent || error != null) {
      map['error'] = Variable<String>(error);
    }
    map['updated_at'] = Variable<DateTime>(updatedAt);
    return map;
  }

  DownloadJobsCompanion toCompanion(bool nullToAbsent) {
    return DownloadJobsCompanion(
      mediaId: Value(mediaId),
      title: Value(title),
      sourceUrl: Value(sourceUrl),
      status: Value(status),
      totalBytes: Value(totalBytes),
      receivedBytes: Value(receivedBytes),
      playlistId: playlistId == null && nullToAbsent
          ? const Value.absent()
          : Value(playlistId),
      error:
          error == null && nullToAbsent ? const Value.absent() : Value(error),
      updatedAt: Value(updatedAt),
    );
  }

  factory DownloadJob.fromJson(Map<String, dynamic> json,
      {ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return DownloadJob(
      mediaId: serializer.fromJson<String>(json['mediaId']),
      title: serializer.fromJson<String>(json['title']),
      sourceUrl: serializer.fromJson<String>(json['sourceUrl']),
      status: serializer.fromJson<String>(json['status']),
      totalBytes: serializer.fromJson<int>(json['totalBytes']),
      receivedBytes: serializer.fromJson<int>(json['receivedBytes']),
      playlistId: serializer.fromJson<String?>(json['playlistId']),
      error: serializer.fromJson<String?>(json['error']),
      updatedAt: serializer.fromJson<DateTime>(json['updatedAt']),
    );
  }
  @override
  Map<String, dynamic> toJson({ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return <String, dynamic>{
      'mediaId': serializer.toJson<String>(mediaId),
      'title': serializer.toJson<String>(title),
      'sourceUrl': serializer.toJson<String>(sourceUrl),
      'status': serializer.toJson<String>(status),
      'totalBytes': serializer.toJson<int>(totalBytes),
      'receivedBytes': serializer.toJson<int>(receivedBytes),
      'playlistId': serializer.toJson<String?>(playlistId),
      'error': serializer.toJson<String?>(error),
      'updatedAt': serializer.toJson<DateTime>(updatedAt),
    };
  }

  DownloadJob copyWith(
          {String? mediaId,
          String? title,
          String? sourceUrl,
          String? status,
          int? totalBytes,
          int? receivedBytes,
          Value<String?> playlistId = const Value.absent(),
          Value<String?> error = const Value.absent(),
          DateTime? updatedAt}) =>
      DownloadJob(
        mediaId: mediaId ?? this.mediaId,
        title: title ?? this.title,
        sourceUrl: sourceUrl ?? this.sourceUrl,
        status: status ?? this.status,
        totalBytes: totalBytes ?? this.totalBytes,
        receivedBytes: receivedBytes ?? this.receivedBytes,
        playlistId: playlistId.present ? playlistId.value : this.playlistId,
        error: error.present ? error.value : this.error,
        updatedAt: updatedAt ?? this.updatedAt,
      );
  DownloadJob copyWithCompanion(DownloadJobsCompanion data) {
    return DownloadJob(
      mediaId: data.mediaId.present ? data.mediaId.value : this.mediaId,
      title: data.title.present ? data.title.value : this.title,
      sourceUrl: data.sourceUrl.present ? data.sourceUrl.value : this.sourceUrl,
      status: data.status.present ? data.status.value : this.status,
      totalBytes:
          data.totalBytes.present ? data.totalBytes.value : this.totalBytes,
      receivedBytes: data.receivedBytes.present
          ? data.receivedBytes.value
          : this.receivedBytes,
      playlistId:
          data.playlistId.present ? data.playlistId.value : this.playlistId,
      error: data.error.present ? data.error.value : this.error,
      updatedAt: data.updatedAt.present ? data.updatedAt.value : this.updatedAt,
    );
  }

  @override
  String toString() {
    return (StringBuffer('DownloadJob(')
          ..write('mediaId: $mediaId, ')
          ..write('title: $title, ')
          ..write('sourceUrl: $sourceUrl, ')
          ..write('status: $status, ')
          ..write('totalBytes: $totalBytes, ')
          ..write('receivedBytes: $receivedBytes, ')
          ..write('playlistId: $playlistId, ')
          ..write('error: $error, ')
          ..write('updatedAt: $updatedAt')
          ..write(')'))
        .toString();
  }

  @override
  int get hashCode => Object.hash(mediaId, title, sourceUrl, status, totalBytes,
      receivedBytes, playlistId, error, updatedAt);
  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is DownloadJob &&
          other.mediaId == this.mediaId &&
          other.title == this.title &&
          other.sourceUrl == this.sourceUrl &&
          other.status == this.status &&
          other.totalBytes == this.totalBytes &&
          other.receivedBytes == this.receivedBytes &&
          other.playlistId == this.playlistId &&
          other.error == this.error &&
          other.updatedAt == this.updatedAt);
}

class DownloadJobsCompanion extends UpdateCompanion<DownloadJob> {
  final Value<String> mediaId;
  final Value<String> title;
  final Value<String> sourceUrl;
  final Value<String> status;
  final Value<int> totalBytes;
  final Value<int> receivedBytes;
  final Value<String?> playlistId;
  final Value<String?> error;
  final Value<DateTime> updatedAt;
  final Value<int> rowid;
  const DownloadJobsCompanion({
    this.mediaId = const Value.absent(),
    this.title = const Value.absent(),
    this.sourceUrl = const Value.absent(),
    this.status = const Value.absent(),
    this.totalBytes = const Value.absent(),
    this.receivedBytes = const Value.absent(),
    this.playlistId = const Value.absent(),
    this.error = const Value.absent(),
    this.updatedAt = const Value.absent(),
    this.rowid = const Value.absent(),
  });
  DownloadJobsCompanion.insert({
    required String mediaId,
    this.title = const Value.absent(),
    required String sourceUrl,
    this.status = const Value.absent(),
    this.totalBytes = const Value.absent(),
    this.receivedBytes = const Value.absent(),
    this.playlistId = const Value.absent(),
    this.error = const Value.absent(),
    this.updatedAt = const Value.absent(),
    this.rowid = const Value.absent(),
  })  : mediaId = Value(mediaId),
        sourceUrl = Value(sourceUrl);
  static Insertable<DownloadJob> custom({
    Expression<String>? mediaId,
    Expression<String>? title,
    Expression<String>? sourceUrl,
    Expression<String>? status,
    Expression<int>? totalBytes,
    Expression<int>? receivedBytes,
    Expression<String>? playlistId,
    Expression<String>? error,
    Expression<DateTime>? updatedAt,
    Expression<int>? rowid,
  }) {
    return RawValuesInsertable({
      if (mediaId != null) 'media_id': mediaId,
      if (title != null) 'title': title,
      if (sourceUrl != null) 'source_url': sourceUrl,
      if (status != null) 'status': status,
      if (totalBytes != null) 'total_bytes': totalBytes,
      if (receivedBytes != null) 'received_bytes': receivedBytes,
      if (playlistId != null) 'playlist_id': playlistId,
      if (error != null) 'error': error,
      if (updatedAt != null) 'updated_at': updatedAt,
      if (rowid != null) 'rowid': rowid,
    });
  }

  DownloadJobsCompanion copyWith(
      {Value<String>? mediaId,
      Value<String>? title,
      Value<String>? sourceUrl,
      Value<String>? status,
      Value<int>? totalBytes,
      Value<int>? receivedBytes,
      Value<String?>? playlistId,
      Value<String?>? error,
      Value<DateTime>? updatedAt,
      Value<int>? rowid}) {
    return DownloadJobsCompanion(
      mediaId: mediaId ?? this.mediaId,
      title: title ?? this.title,
      sourceUrl: sourceUrl ?? this.sourceUrl,
      status: status ?? this.status,
      totalBytes: totalBytes ?? this.totalBytes,
      receivedBytes: receivedBytes ?? this.receivedBytes,
      playlistId: playlistId ?? this.playlistId,
      error: error ?? this.error,
      updatedAt: updatedAt ?? this.updatedAt,
      rowid: rowid ?? this.rowid,
    );
  }

  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    if (mediaId.present) {
      map['media_id'] = Variable<String>(mediaId.value);
    }
    if (title.present) {
      map['title'] = Variable<String>(title.value);
    }
    if (sourceUrl.present) {
      map['source_url'] = Variable<String>(sourceUrl.value);
    }
    if (status.present) {
      map['status'] = Variable<String>(status.value);
    }
    if (totalBytes.present) {
      map['total_bytes'] = Variable<int>(totalBytes.value);
    }
    if (receivedBytes.present) {
      map['received_bytes'] = Variable<int>(receivedBytes.value);
    }
    if (playlistId.present) {
      map['playlist_id'] = Variable<String>(playlistId.value);
    }
    if (error.present) {
      map['error'] = Variable<String>(error.value);
    }
    if (updatedAt.present) {
      map['updated_at'] = Variable<DateTime>(updatedAt.value);
    }
    if (rowid.present) {
      map['rowid'] = Variable<int>(rowid.value);
    }
    return map;
  }

  @override
  String toString() {
    return (StringBuffer('DownloadJobsCompanion(')
          ..write('mediaId: $mediaId, ')
          ..write('title: $title, ')
          ..write('sourceUrl: $sourceUrl, ')
          ..write('status: $status, ')
          ..write('totalBytes: $totalBytes, ')
          ..write('receivedBytes: $receivedBytes, ')
          ..write('playlistId: $playlistId, ')
          ..write('error: $error, ')
          ..write('updatedAt: $updatedAt, ')
          ..write('rowid: $rowid')
          ..write(')'))
        .toString();
  }
}

class $DownloadedTracksTable extends DownloadedTracks
    with TableInfo<$DownloadedTracksTable, DownloadedTrack> {
  @override
  final GeneratedDatabase attachedDatabase;
  final String? _alias;
  $DownloadedTracksTable(this.attachedDatabase, [this._alias]);
  static const VerificationMeta _mediaIdMeta =
      const VerificationMeta('mediaId');
  @override
  late final GeneratedColumn<String> mediaId = GeneratedColumn<String>(
      'media_id', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _localPathMeta =
      const VerificationMeta('localPath');
  @override
  late final GeneratedColumn<String> localPath = GeneratedColumn<String>(
      'local_path', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _bytesMeta = const VerificationMeta('bytes');
  @override
  late final GeneratedColumn<int> bytes = GeneratedColumn<int>(
      'bytes', aliasedName, false,
      type: DriftSqlType.int,
      requiredDuringInsert: false,
      defaultValue: const Constant(0));
  static const VerificationMeta _sha256Meta = const VerificationMeta('sha256');
  @override
  late final GeneratedColumn<String> sha256 = GeneratedColumn<String>(
      'sha256', aliasedName, true,
      type: DriftSqlType.string, requiredDuringInsert: false);
  static const VerificationMeta _completedAtMeta =
      const VerificationMeta('completedAt');
  @override
  late final GeneratedColumn<DateTime> completedAt = GeneratedColumn<DateTime>(
      'completed_at', aliasedName, false,
      type: DriftSqlType.dateTime,
      requiredDuringInsert: false,
      defaultValue: currentDateAndTime);
  @override
  List<GeneratedColumn> get $columns =>
      [mediaId, localPath, bytes, sha256, completedAt];
  @override
  String get aliasedName => _alias ?? actualTableName;
  @override
  String get actualTableName => $name;
  static const String $name = 'downloaded_tracks';
  @override
  VerificationContext validateIntegrity(Insertable<DownloadedTrack> instance,
      {bool isInserting = false}) {
    final context = VerificationContext();
    final data = instance.toColumns(true);
    if (data.containsKey('media_id')) {
      context.handle(_mediaIdMeta,
          mediaId.isAcceptableOrUnknown(data['media_id']!, _mediaIdMeta));
    } else if (isInserting) {
      context.missing(_mediaIdMeta);
    }
    if (data.containsKey('local_path')) {
      context.handle(_localPathMeta,
          localPath.isAcceptableOrUnknown(data['local_path']!, _localPathMeta));
    } else if (isInserting) {
      context.missing(_localPathMeta);
    }
    if (data.containsKey('bytes')) {
      context.handle(
          _bytesMeta, bytes.isAcceptableOrUnknown(data['bytes']!, _bytesMeta));
    }
    if (data.containsKey('sha256')) {
      context.handle(_sha256Meta,
          sha256.isAcceptableOrUnknown(data['sha256']!, _sha256Meta));
    }
    if (data.containsKey('completed_at')) {
      context.handle(
          _completedAtMeta,
          completedAt.isAcceptableOrUnknown(
              data['completed_at']!, _completedAtMeta));
    }
    return context;
  }

  @override
  Set<GeneratedColumn> get $primaryKey => {mediaId};
  @override
  DownloadedTrack map(Map<String, dynamic> data, {String? tablePrefix}) {
    final effectivePrefix = tablePrefix != null ? '$tablePrefix.' : '';
    return DownloadedTrack(
      mediaId: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}media_id'])!,
      localPath: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}local_path'])!,
      bytes: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}bytes'])!,
      sha256: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}sha256']),
      completedAt: attachedDatabase.typeMapping
          .read(DriftSqlType.dateTime, data['${effectivePrefix}completed_at'])!,
    );
  }

  @override
  $DownloadedTracksTable createAlias(String alias) {
    return $DownloadedTracksTable(attachedDatabase, alias);
  }
}

class DownloadedTrack extends DataClass implements Insertable<DownloadedTrack> {
  final String mediaId;
  final String localPath;
  final int bytes;
  final String? sha256;
  final DateTime completedAt;
  const DownloadedTrack(
      {required this.mediaId,
      required this.localPath,
      required this.bytes,
      this.sha256,
      required this.completedAt});
  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    map['media_id'] = Variable<String>(mediaId);
    map['local_path'] = Variable<String>(localPath);
    map['bytes'] = Variable<int>(bytes);
    if (!nullToAbsent || sha256 != null) {
      map['sha256'] = Variable<String>(sha256);
    }
    map['completed_at'] = Variable<DateTime>(completedAt);
    return map;
  }

  DownloadedTracksCompanion toCompanion(bool nullToAbsent) {
    return DownloadedTracksCompanion(
      mediaId: Value(mediaId),
      localPath: Value(localPath),
      bytes: Value(bytes),
      sha256:
          sha256 == null && nullToAbsent ? const Value.absent() : Value(sha256),
      completedAt: Value(completedAt),
    );
  }

  factory DownloadedTrack.fromJson(Map<String, dynamic> json,
      {ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return DownloadedTrack(
      mediaId: serializer.fromJson<String>(json['mediaId']),
      localPath: serializer.fromJson<String>(json['localPath']),
      bytes: serializer.fromJson<int>(json['bytes']),
      sha256: serializer.fromJson<String?>(json['sha256']),
      completedAt: serializer.fromJson<DateTime>(json['completedAt']),
    );
  }
  @override
  Map<String, dynamic> toJson({ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return <String, dynamic>{
      'mediaId': serializer.toJson<String>(mediaId),
      'localPath': serializer.toJson<String>(localPath),
      'bytes': serializer.toJson<int>(bytes),
      'sha256': serializer.toJson<String?>(sha256),
      'completedAt': serializer.toJson<DateTime>(completedAt),
    };
  }

  DownloadedTrack copyWith(
          {String? mediaId,
          String? localPath,
          int? bytes,
          Value<String?> sha256 = const Value.absent(),
          DateTime? completedAt}) =>
      DownloadedTrack(
        mediaId: mediaId ?? this.mediaId,
        localPath: localPath ?? this.localPath,
        bytes: bytes ?? this.bytes,
        sha256: sha256.present ? sha256.value : this.sha256,
        completedAt: completedAt ?? this.completedAt,
      );
  DownloadedTrack copyWithCompanion(DownloadedTracksCompanion data) {
    return DownloadedTrack(
      mediaId: data.mediaId.present ? data.mediaId.value : this.mediaId,
      localPath: data.localPath.present ? data.localPath.value : this.localPath,
      bytes: data.bytes.present ? data.bytes.value : this.bytes,
      sha256: data.sha256.present ? data.sha256.value : this.sha256,
      completedAt:
          data.completedAt.present ? data.completedAt.value : this.completedAt,
    );
  }

  @override
  String toString() {
    return (StringBuffer('DownloadedTrack(')
          ..write('mediaId: $mediaId, ')
          ..write('localPath: $localPath, ')
          ..write('bytes: $bytes, ')
          ..write('sha256: $sha256, ')
          ..write('completedAt: $completedAt')
          ..write(')'))
        .toString();
  }

  @override
  int get hashCode =>
      Object.hash(mediaId, localPath, bytes, sha256, completedAt);
  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is DownloadedTrack &&
          other.mediaId == this.mediaId &&
          other.localPath == this.localPath &&
          other.bytes == this.bytes &&
          other.sha256 == this.sha256 &&
          other.completedAt == this.completedAt);
}

class DownloadedTracksCompanion extends UpdateCompanion<DownloadedTrack> {
  final Value<String> mediaId;
  final Value<String> localPath;
  final Value<int> bytes;
  final Value<String?> sha256;
  final Value<DateTime> completedAt;
  final Value<int> rowid;
  const DownloadedTracksCompanion({
    this.mediaId = const Value.absent(),
    this.localPath = const Value.absent(),
    this.bytes = const Value.absent(),
    this.sha256 = const Value.absent(),
    this.completedAt = const Value.absent(),
    this.rowid = const Value.absent(),
  });
  DownloadedTracksCompanion.insert({
    required String mediaId,
    required String localPath,
    this.bytes = const Value.absent(),
    this.sha256 = const Value.absent(),
    this.completedAt = const Value.absent(),
    this.rowid = const Value.absent(),
  })  : mediaId = Value(mediaId),
        localPath = Value(localPath);
  static Insertable<DownloadedTrack> custom({
    Expression<String>? mediaId,
    Expression<String>? localPath,
    Expression<int>? bytes,
    Expression<String>? sha256,
    Expression<DateTime>? completedAt,
    Expression<int>? rowid,
  }) {
    return RawValuesInsertable({
      if (mediaId != null) 'media_id': mediaId,
      if (localPath != null) 'local_path': localPath,
      if (bytes != null) 'bytes': bytes,
      if (sha256 != null) 'sha256': sha256,
      if (completedAt != null) 'completed_at': completedAt,
      if (rowid != null) 'rowid': rowid,
    });
  }

  DownloadedTracksCompanion copyWith(
      {Value<String>? mediaId,
      Value<String>? localPath,
      Value<int>? bytes,
      Value<String?>? sha256,
      Value<DateTime>? completedAt,
      Value<int>? rowid}) {
    return DownloadedTracksCompanion(
      mediaId: mediaId ?? this.mediaId,
      localPath: localPath ?? this.localPath,
      bytes: bytes ?? this.bytes,
      sha256: sha256 ?? this.sha256,
      completedAt: completedAt ?? this.completedAt,
      rowid: rowid ?? this.rowid,
    );
  }

  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    if (mediaId.present) {
      map['media_id'] = Variable<String>(mediaId.value);
    }
    if (localPath.present) {
      map['local_path'] = Variable<String>(localPath.value);
    }
    if (bytes.present) {
      map['bytes'] = Variable<int>(bytes.value);
    }
    if (sha256.present) {
      map['sha256'] = Variable<String>(sha256.value);
    }
    if (completedAt.present) {
      map['completed_at'] = Variable<DateTime>(completedAt.value);
    }
    if (rowid.present) {
      map['rowid'] = Variable<int>(rowid.value);
    }
    return map;
  }

  @override
  String toString() {
    return (StringBuffer('DownloadedTracksCompanion(')
          ..write('mediaId: $mediaId, ')
          ..write('localPath: $localPath, ')
          ..write('bytes: $bytes, ')
          ..write('sha256: $sha256, ')
          ..write('completedAt: $completedAt, ')
          ..write('rowid: $rowid')
          ..write(')'))
        .toString();
  }
}

class $PendingMutationsTable extends PendingMutations
    with TableInfo<$PendingMutationsTable, PendingMutation> {
  @override
  final GeneratedDatabase attachedDatabase;
  final String? _alias;
  $PendingMutationsTable(this.attachedDatabase, [this._alias]);
  static const VerificationMeta _idempotencyKeyMeta =
      const VerificationMeta('idempotencyKey');
  @override
  late final GeneratedColumn<String> idempotencyKey = GeneratedColumn<String>(
      'idempotency_key', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _kindMeta = const VerificationMeta('kind');
  @override
  late final GeneratedColumn<String> kind = GeneratedColumn<String>(
      'kind', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _methodMeta = const VerificationMeta('method');
  @override
  late final GeneratedColumn<String> method = GeneratedColumn<String>(
      'method', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _pathMeta = const VerificationMeta('path');
  @override
  late final GeneratedColumn<String> path = GeneratedColumn<String>(
      'path', aliasedName, false,
      type: DriftSqlType.string, requiredDuringInsert: true);
  static const VerificationMeta _bodyJsonMeta =
      const VerificationMeta('bodyJson');
  @override
  late final GeneratedColumn<String> bodyJson = GeneratedColumn<String>(
      'body_json', aliasedName, false,
      type: DriftSqlType.string,
      requiredDuringInsert: false,
      defaultValue: const Constant(''));
  static const VerificationMeta _statusMeta = const VerificationMeta('status');
  @override
  late final GeneratedColumn<String> status = GeneratedColumn<String>(
      'status', aliasedName, false,
      type: DriftSqlType.string,
      requiredDuringInsert: false,
      defaultValue: const Constant('pending'));
  static const VerificationMeta _attemptsMeta =
      const VerificationMeta('attempts');
  @override
  late final GeneratedColumn<int> attempts = GeneratedColumn<int>(
      'attempts', aliasedName, false,
      type: DriftSqlType.int,
      requiredDuringInsert: false,
      defaultValue: const Constant(0));
  static const VerificationMeta _clientClockMeta =
      const VerificationMeta('clientClock');
  @override
  late final GeneratedColumn<int> clientClock = GeneratedColumn<int>(
      'client_clock', aliasedName, false,
      type: DriftSqlType.int, requiredDuringInsert: true);
  static const VerificationMeta _nextAttemptAtMeta =
      const VerificationMeta('nextAttemptAt');
  @override
  late final GeneratedColumn<int> nextAttemptAt = GeneratedColumn<int>(
      'next_attempt_at', aliasedName, false,
      type: DriftSqlType.int,
      requiredDuringInsert: false,
      defaultValue: const Constant(0));
  static const VerificationMeta _priorityMeta =
      const VerificationMeta('priority');
  @override
  late final GeneratedColumn<int> priority = GeneratedColumn<int>(
      'priority', aliasedName, false,
      type: DriftSqlType.int,
      requiredDuringInsert: false,
      defaultValue: const Constant(1));
  static const VerificationMeta _errorMeta = const VerificationMeta('error');
  @override
  late final GeneratedColumn<String> error = GeneratedColumn<String>(
      'error', aliasedName, true,
      type: DriftSqlType.string, requiredDuringInsert: false);
  static const VerificationMeta _createdAtMeta =
      const VerificationMeta('createdAt');
  @override
  late final GeneratedColumn<DateTime> createdAt = GeneratedColumn<DateTime>(
      'created_at', aliasedName, false,
      type: DriftSqlType.dateTime,
      requiredDuringInsert: false,
      defaultValue: currentDateAndTime);
  @override
  List<GeneratedColumn> get $columns => [
        idempotencyKey,
        kind,
        method,
        path,
        bodyJson,
        status,
        attempts,
        clientClock,
        nextAttemptAt,
        priority,
        error,
        createdAt
      ];
  @override
  String get aliasedName => _alias ?? actualTableName;
  @override
  String get actualTableName => $name;
  static const String $name = 'pending_mutations';
  @override
  VerificationContext validateIntegrity(Insertable<PendingMutation> instance,
      {bool isInserting = false}) {
    final context = VerificationContext();
    final data = instance.toColumns(true);
    if (data.containsKey('idempotency_key')) {
      context.handle(
          _idempotencyKeyMeta,
          idempotencyKey.isAcceptableOrUnknown(
              data['idempotency_key']!, _idempotencyKeyMeta));
    } else if (isInserting) {
      context.missing(_idempotencyKeyMeta);
    }
    if (data.containsKey('kind')) {
      context.handle(
          _kindMeta, kind.isAcceptableOrUnknown(data['kind']!, _kindMeta));
    } else if (isInserting) {
      context.missing(_kindMeta);
    }
    if (data.containsKey('method')) {
      context.handle(_methodMeta,
          method.isAcceptableOrUnknown(data['method']!, _methodMeta));
    } else if (isInserting) {
      context.missing(_methodMeta);
    }
    if (data.containsKey('path')) {
      context.handle(
          _pathMeta, path.isAcceptableOrUnknown(data['path']!, _pathMeta));
    } else if (isInserting) {
      context.missing(_pathMeta);
    }
    if (data.containsKey('body_json')) {
      context.handle(_bodyJsonMeta,
          bodyJson.isAcceptableOrUnknown(data['body_json']!, _bodyJsonMeta));
    }
    if (data.containsKey('status')) {
      context.handle(_statusMeta,
          status.isAcceptableOrUnknown(data['status']!, _statusMeta));
    }
    if (data.containsKey('attempts')) {
      context.handle(_attemptsMeta,
          attempts.isAcceptableOrUnknown(data['attempts']!, _attemptsMeta));
    }
    if (data.containsKey('client_clock')) {
      context.handle(
          _clientClockMeta,
          clientClock.isAcceptableOrUnknown(
              data['client_clock']!, _clientClockMeta));
    } else if (isInserting) {
      context.missing(_clientClockMeta);
    }
    if (data.containsKey('next_attempt_at')) {
      context.handle(
          _nextAttemptAtMeta,
          nextAttemptAt.isAcceptableOrUnknown(
              data['next_attempt_at']!, _nextAttemptAtMeta));
    }
    if (data.containsKey('priority')) {
      context.handle(_priorityMeta,
          priority.isAcceptableOrUnknown(data['priority']!, _priorityMeta));
    }
    if (data.containsKey('error')) {
      context.handle(
          _errorMeta, error.isAcceptableOrUnknown(data['error']!, _errorMeta));
    }
    if (data.containsKey('created_at')) {
      context.handle(_createdAtMeta,
          createdAt.isAcceptableOrUnknown(data['created_at']!, _createdAtMeta));
    }
    return context;
  }

  @override
  Set<GeneratedColumn> get $primaryKey => {idempotencyKey};
  @override
  PendingMutation map(Map<String, dynamic> data, {String? tablePrefix}) {
    final effectivePrefix = tablePrefix != null ? '$tablePrefix.' : '';
    return PendingMutation(
      idempotencyKey: attachedDatabase.typeMapping.read(
          DriftSqlType.string, data['${effectivePrefix}idempotency_key'])!,
      kind: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}kind'])!,
      method: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}method'])!,
      path: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}path'])!,
      bodyJson: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}body_json'])!,
      status: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}status'])!,
      attempts: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}attempts'])!,
      clientClock: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}client_clock'])!,
      nextAttemptAt: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}next_attempt_at'])!,
      priority: attachedDatabase.typeMapping
          .read(DriftSqlType.int, data['${effectivePrefix}priority'])!,
      error: attachedDatabase.typeMapping
          .read(DriftSqlType.string, data['${effectivePrefix}error']),
      createdAt: attachedDatabase.typeMapping
          .read(DriftSqlType.dateTime, data['${effectivePrefix}created_at'])!,
    );
  }

  @override
  $PendingMutationsTable createAlias(String alias) {
    return $PendingMutationsTable(attachedDatabase, alias);
  }
}

class PendingMutation extends DataClass implements Insertable<PendingMutation> {
  /// UUIDv7 — also the Idempotency-Key. Monotonic, so ordering by it == client
  /// clock order.
  final String idempotencyKey;
  final String kind;
  final String method;
  final String path;

  /// JSON request body (empty for DELETEs).
  final String bodyJson;
  final String status;
  final int attempts;

  /// Monotonic client clock; replay order. Lower = older.
  final int clientClock;

  /// When the next replay attempt is due (ms since epoch); backoff schedule.
  final int nextAttemptAt;

  /// Priority for eviction: higher survives. likes(2) > most(1) > impression(0).
  final int priority;
  final String? error;
  final DateTime createdAt;
  const PendingMutation(
      {required this.idempotencyKey,
      required this.kind,
      required this.method,
      required this.path,
      required this.bodyJson,
      required this.status,
      required this.attempts,
      required this.clientClock,
      required this.nextAttemptAt,
      required this.priority,
      this.error,
      required this.createdAt});
  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    map['idempotency_key'] = Variable<String>(idempotencyKey);
    map['kind'] = Variable<String>(kind);
    map['method'] = Variable<String>(method);
    map['path'] = Variable<String>(path);
    map['body_json'] = Variable<String>(bodyJson);
    map['status'] = Variable<String>(status);
    map['attempts'] = Variable<int>(attempts);
    map['client_clock'] = Variable<int>(clientClock);
    map['next_attempt_at'] = Variable<int>(nextAttemptAt);
    map['priority'] = Variable<int>(priority);
    if (!nullToAbsent || error != null) {
      map['error'] = Variable<String>(error);
    }
    map['created_at'] = Variable<DateTime>(createdAt);
    return map;
  }

  PendingMutationsCompanion toCompanion(bool nullToAbsent) {
    return PendingMutationsCompanion(
      idempotencyKey: Value(idempotencyKey),
      kind: Value(kind),
      method: Value(method),
      path: Value(path),
      bodyJson: Value(bodyJson),
      status: Value(status),
      attempts: Value(attempts),
      clientClock: Value(clientClock),
      nextAttemptAt: Value(nextAttemptAt),
      priority: Value(priority),
      error:
          error == null && nullToAbsent ? const Value.absent() : Value(error),
      createdAt: Value(createdAt),
    );
  }

  factory PendingMutation.fromJson(Map<String, dynamic> json,
      {ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return PendingMutation(
      idempotencyKey: serializer.fromJson<String>(json['idempotencyKey']),
      kind: serializer.fromJson<String>(json['kind']),
      method: serializer.fromJson<String>(json['method']),
      path: serializer.fromJson<String>(json['path']),
      bodyJson: serializer.fromJson<String>(json['bodyJson']),
      status: serializer.fromJson<String>(json['status']),
      attempts: serializer.fromJson<int>(json['attempts']),
      clientClock: serializer.fromJson<int>(json['clientClock']),
      nextAttemptAt: serializer.fromJson<int>(json['nextAttemptAt']),
      priority: serializer.fromJson<int>(json['priority']),
      error: serializer.fromJson<String?>(json['error']),
      createdAt: serializer.fromJson<DateTime>(json['createdAt']),
    );
  }
  @override
  Map<String, dynamic> toJson({ValueSerializer? serializer}) {
    serializer ??= driftRuntimeOptions.defaultSerializer;
    return <String, dynamic>{
      'idempotencyKey': serializer.toJson<String>(idempotencyKey),
      'kind': serializer.toJson<String>(kind),
      'method': serializer.toJson<String>(method),
      'path': serializer.toJson<String>(path),
      'bodyJson': serializer.toJson<String>(bodyJson),
      'status': serializer.toJson<String>(status),
      'attempts': serializer.toJson<int>(attempts),
      'clientClock': serializer.toJson<int>(clientClock),
      'nextAttemptAt': serializer.toJson<int>(nextAttemptAt),
      'priority': serializer.toJson<int>(priority),
      'error': serializer.toJson<String?>(error),
      'createdAt': serializer.toJson<DateTime>(createdAt),
    };
  }

  PendingMutation copyWith(
          {String? idempotencyKey,
          String? kind,
          String? method,
          String? path,
          String? bodyJson,
          String? status,
          int? attempts,
          int? clientClock,
          int? nextAttemptAt,
          int? priority,
          Value<String?> error = const Value.absent(),
          DateTime? createdAt}) =>
      PendingMutation(
        idempotencyKey: idempotencyKey ?? this.idempotencyKey,
        kind: kind ?? this.kind,
        method: method ?? this.method,
        path: path ?? this.path,
        bodyJson: bodyJson ?? this.bodyJson,
        status: status ?? this.status,
        attempts: attempts ?? this.attempts,
        clientClock: clientClock ?? this.clientClock,
        nextAttemptAt: nextAttemptAt ?? this.nextAttemptAt,
        priority: priority ?? this.priority,
        error: error.present ? error.value : this.error,
        createdAt: createdAt ?? this.createdAt,
      );
  PendingMutation copyWithCompanion(PendingMutationsCompanion data) {
    return PendingMutation(
      idempotencyKey: data.idempotencyKey.present
          ? data.idempotencyKey.value
          : this.idempotencyKey,
      kind: data.kind.present ? data.kind.value : this.kind,
      method: data.method.present ? data.method.value : this.method,
      path: data.path.present ? data.path.value : this.path,
      bodyJson: data.bodyJson.present ? data.bodyJson.value : this.bodyJson,
      status: data.status.present ? data.status.value : this.status,
      attempts: data.attempts.present ? data.attempts.value : this.attempts,
      clientClock:
          data.clientClock.present ? data.clientClock.value : this.clientClock,
      nextAttemptAt: data.nextAttemptAt.present
          ? data.nextAttemptAt.value
          : this.nextAttemptAt,
      priority: data.priority.present ? data.priority.value : this.priority,
      error: data.error.present ? data.error.value : this.error,
      createdAt: data.createdAt.present ? data.createdAt.value : this.createdAt,
    );
  }

  @override
  String toString() {
    return (StringBuffer('PendingMutation(')
          ..write('idempotencyKey: $idempotencyKey, ')
          ..write('kind: $kind, ')
          ..write('method: $method, ')
          ..write('path: $path, ')
          ..write('bodyJson: $bodyJson, ')
          ..write('status: $status, ')
          ..write('attempts: $attempts, ')
          ..write('clientClock: $clientClock, ')
          ..write('nextAttemptAt: $nextAttemptAt, ')
          ..write('priority: $priority, ')
          ..write('error: $error, ')
          ..write('createdAt: $createdAt')
          ..write(')'))
        .toString();
  }

  @override
  int get hashCode => Object.hash(idempotencyKey, kind, method, path, bodyJson,
      status, attempts, clientClock, nextAttemptAt, priority, error, createdAt);
  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is PendingMutation &&
          other.idempotencyKey == this.idempotencyKey &&
          other.kind == this.kind &&
          other.method == this.method &&
          other.path == this.path &&
          other.bodyJson == this.bodyJson &&
          other.status == this.status &&
          other.attempts == this.attempts &&
          other.clientClock == this.clientClock &&
          other.nextAttemptAt == this.nextAttemptAt &&
          other.priority == this.priority &&
          other.error == this.error &&
          other.createdAt == this.createdAt);
}

class PendingMutationsCompanion extends UpdateCompanion<PendingMutation> {
  final Value<String> idempotencyKey;
  final Value<String> kind;
  final Value<String> method;
  final Value<String> path;
  final Value<String> bodyJson;
  final Value<String> status;
  final Value<int> attempts;
  final Value<int> clientClock;
  final Value<int> nextAttemptAt;
  final Value<int> priority;
  final Value<String?> error;
  final Value<DateTime> createdAt;
  final Value<int> rowid;
  const PendingMutationsCompanion({
    this.idempotencyKey = const Value.absent(),
    this.kind = const Value.absent(),
    this.method = const Value.absent(),
    this.path = const Value.absent(),
    this.bodyJson = const Value.absent(),
    this.status = const Value.absent(),
    this.attempts = const Value.absent(),
    this.clientClock = const Value.absent(),
    this.nextAttemptAt = const Value.absent(),
    this.priority = const Value.absent(),
    this.error = const Value.absent(),
    this.createdAt = const Value.absent(),
    this.rowid = const Value.absent(),
  });
  PendingMutationsCompanion.insert({
    required String idempotencyKey,
    required String kind,
    required String method,
    required String path,
    this.bodyJson = const Value.absent(),
    this.status = const Value.absent(),
    this.attempts = const Value.absent(),
    required int clientClock,
    this.nextAttemptAt = const Value.absent(),
    this.priority = const Value.absent(),
    this.error = const Value.absent(),
    this.createdAt = const Value.absent(),
    this.rowid = const Value.absent(),
  })  : idempotencyKey = Value(idempotencyKey),
        kind = Value(kind),
        method = Value(method),
        path = Value(path),
        clientClock = Value(clientClock);
  static Insertable<PendingMutation> custom({
    Expression<String>? idempotencyKey,
    Expression<String>? kind,
    Expression<String>? method,
    Expression<String>? path,
    Expression<String>? bodyJson,
    Expression<String>? status,
    Expression<int>? attempts,
    Expression<int>? clientClock,
    Expression<int>? nextAttemptAt,
    Expression<int>? priority,
    Expression<String>? error,
    Expression<DateTime>? createdAt,
    Expression<int>? rowid,
  }) {
    return RawValuesInsertable({
      if (idempotencyKey != null) 'idempotency_key': idempotencyKey,
      if (kind != null) 'kind': kind,
      if (method != null) 'method': method,
      if (path != null) 'path': path,
      if (bodyJson != null) 'body_json': bodyJson,
      if (status != null) 'status': status,
      if (attempts != null) 'attempts': attempts,
      if (clientClock != null) 'client_clock': clientClock,
      if (nextAttemptAt != null) 'next_attempt_at': nextAttemptAt,
      if (priority != null) 'priority': priority,
      if (error != null) 'error': error,
      if (createdAt != null) 'created_at': createdAt,
      if (rowid != null) 'rowid': rowid,
    });
  }

  PendingMutationsCompanion copyWith(
      {Value<String>? idempotencyKey,
      Value<String>? kind,
      Value<String>? method,
      Value<String>? path,
      Value<String>? bodyJson,
      Value<String>? status,
      Value<int>? attempts,
      Value<int>? clientClock,
      Value<int>? nextAttemptAt,
      Value<int>? priority,
      Value<String?>? error,
      Value<DateTime>? createdAt,
      Value<int>? rowid}) {
    return PendingMutationsCompanion(
      idempotencyKey: idempotencyKey ?? this.idempotencyKey,
      kind: kind ?? this.kind,
      method: method ?? this.method,
      path: path ?? this.path,
      bodyJson: bodyJson ?? this.bodyJson,
      status: status ?? this.status,
      attempts: attempts ?? this.attempts,
      clientClock: clientClock ?? this.clientClock,
      nextAttemptAt: nextAttemptAt ?? this.nextAttemptAt,
      priority: priority ?? this.priority,
      error: error ?? this.error,
      createdAt: createdAt ?? this.createdAt,
      rowid: rowid ?? this.rowid,
    );
  }

  @override
  Map<String, Expression> toColumns(bool nullToAbsent) {
    final map = <String, Expression>{};
    if (idempotencyKey.present) {
      map['idempotency_key'] = Variable<String>(idempotencyKey.value);
    }
    if (kind.present) {
      map['kind'] = Variable<String>(kind.value);
    }
    if (method.present) {
      map['method'] = Variable<String>(method.value);
    }
    if (path.present) {
      map['path'] = Variable<String>(path.value);
    }
    if (bodyJson.present) {
      map['body_json'] = Variable<String>(bodyJson.value);
    }
    if (status.present) {
      map['status'] = Variable<String>(status.value);
    }
    if (attempts.present) {
      map['attempts'] = Variable<int>(attempts.value);
    }
    if (clientClock.present) {
      map['client_clock'] = Variable<int>(clientClock.value);
    }
    if (nextAttemptAt.present) {
      map['next_attempt_at'] = Variable<int>(nextAttemptAt.value);
    }
    if (priority.present) {
      map['priority'] = Variable<int>(priority.value);
    }
    if (error.present) {
      map['error'] = Variable<String>(error.value);
    }
    if (createdAt.present) {
      map['created_at'] = Variable<DateTime>(createdAt.value);
    }
    if (rowid.present) {
      map['rowid'] = Variable<int>(rowid.value);
    }
    return map;
  }

  @override
  String toString() {
    return (StringBuffer('PendingMutationsCompanion(')
          ..write('idempotencyKey: $idempotencyKey, ')
          ..write('kind: $kind, ')
          ..write('method: $method, ')
          ..write('path: $path, ')
          ..write('bodyJson: $bodyJson, ')
          ..write('status: $status, ')
          ..write('attempts: $attempts, ')
          ..write('clientClock: $clientClock, ')
          ..write('nextAttemptAt: $nextAttemptAt, ')
          ..write('priority: $priority, ')
          ..write('error: $error, ')
          ..write('createdAt: $createdAt, ')
          ..write('rowid: $rowid')
          ..write(')'))
        .toString();
  }
}

abstract class _$SunflowerDatabase extends GeneratedDatabase {
  _$SunflowerDatabase(QueryExecutor e) : super(e);
  $SunflowerDatabaseManager get managers => $SunflowerDatabaseManager(this);
  late final $LookaheadCacheTable lookaheadCache = $LookaheadCacheTable(this);
  late final $RecentPlaysTable recentPlays = $RecentPlaysTable(this);
  late final $HomeCacheTable homeCache = $HomeCacheTable(this);
  late final $DownloadJobsTable downloadJobs = $DownloadJobsTable(this);
  late final $DownloadedTracksTable downloadedTracks =
      $DownloadedTracksTable(this);
  late final $PendingMutationsTable pendingMutations =
      $PendingMutationsTable(this);
  @override
  Iterable<TableInfo<Table, Object?>> get allTables =>
      allSchemaEntities.whereType<TableInfo<Table, Object?>>();
  @override
  List<DatabaseSchemaEntity> get allSchemaEntities => [
        lookaheadCache,
        recentPlays,
        homeCache,
        downloadJobs,
        downloadedTracks,
        pendingMutations
      ];
}

typedef $$LookaheadCacheTableCreateCompanionBuilder = LookaheadCacheCompanion
    Function({
  required String queueId,
  required int position,
  required String mediaId,
  Value<String> title,
  Value<String> artistsJson,
  Value<int> durationMs,
  Value<String> source,
  Value<String?> streamUrl,
  Value<DateTime?> streamExpiresAt,
  Value<String?> mimeType,
  Value<DateTime> cachedAt,
  Value<int> rowid,
});
typedef $$LookaheadCacheTableUpdateCompanionBuilder = LookaheadCacheCompanion
    Function({
  Value<String> queueId,
  Value<int> position,
  Value<String> mediaId,
  Value<String> title,
  Value<String> artistsJson,
  Value<int> durationMs,
  Value<String> source,
  Value<String?> streamUrl,
  Value<DateTime?> streamExpiresAt,
  Value<String?> mimeType,
  Value<DateTime> cachedAt,
  Value<int> rowid,
});

class $$LookaheadCacheTableFilterComposer
    extends Composer<_$SunflowerDatabase, $LookaheadCacheTable> {
  $$LookaheadCacheTableFilterComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnFilters<String> get queueId => $composableBuilder(
      column: $table.queueId, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get position => $composableBuilder(
      column: $table.position, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get mediaId => $composableBuilder(
      column: $table.mediaId, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get title => $composableBuilder(
      column: $table.title, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get artistsJson => $composableBuilder(
      column: $table.artistsJson, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get durationMs => $composableBuilder(
      column: $table.durationMs, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get source => $composableBuilder(
      column: $table.source, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get streamUrl => $composableBuilder(
      column: $table.streamUrl, builder: (column) => ColumnFilters(column));

  ColumnFilters<DateTime> get streamExpiresAt => $composableBuilder(
      column: $table.streamExpiresAt,
      builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get mimeType => $composableBuilder(
      column: $table.mimeType, builder: (column) => ColumnFilters(column));

  ColumnFilters<DateTime> get cachedAt => $composableBuilder(
      column: $table.cachedAt, builder: (column) => ColumnFilters(column));
}

class $$LookaheadCacheTableOrderingComposer
    extends Composer<_$SunflowerDatabase, $LookaheadCacheTable> {
  $$LookaheadCacheTableOrderingComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnOrderings<String> get queueId => $composableBuilder(
      column: $table.queueId, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get position => $composableBuilder(
      column: $table.position, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get mediaId => $composableBuilder(
      column: $table.mediaId, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get title => $composableBuilder(
      column: $table.title, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get artistsJson => $composableBuilder(
      column: $table.artistsJson, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get durationMs => $composableBuilder(
      column: $table.durationMs, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get source => $composableBuilder(
      column: $table.source, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get streamUrl => $composableBuilder(
      column: $table.streamUrl, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<DateTime> get streamExpiresAt => $composableBuilder(
      column: $table.streamExpiresAt,
      builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get mimeType => $composableBuilder(
      column: $table.mimeType, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<DateTime> get cachedAt => $composableBuilder(
      column: $table.cachedAt, builder: (column) => ColumnOrderings(column));
}

class $$LookaheadCacheTableAnnotationComposer
    extends Composer<_$SunflowerDatabase, $LookaheadCacheTable> {
  $$LookaheadCacheTableAnnotationComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  GeneratedColumn<String> get queueId =>
      $composableBuilder(column: $table.queueId, builder: (column) => column);

  GeneratedColumn<int> get position =>
      $composableBuilder(column: $table.position, builder: (column) => column);

  GeneratedColumn<String> get mediaId =>
      $composableBuilder(column: $table.mediaId, builder: (column) => column);

  GeneratedColumn<String> get title =>
      $composableBuilder(column: $table.title, builder: (column) => column);

  GeneratedColumn<String> get artistsJson => $composableBuilder(
      column: $table.artistsJson, builder: (column) => column);

  GeneratedColumn<int> get durationMs => $composableBuilder(
      column: $table.durationMs, builder: (column) => column);

  GeneratedColumn<String> get source =>
      $composableBuilder(column: $table.source, builder: (column) => column);

  GeneratedColumn<String> get streamUrl =>
      $composableBuilder(column: $table.streamUrl, builder: (column) => column);

  GeneratedColumn<DateTime> get streamExpiresAt => $composableBuilder(
      column: $table.streamExpiresAt, builder: (column) => column);

  GeneratedColumn<String> get mimeType =>
      $composableBuilder(column: $table.mimeType, builder: (column) => column);

  GeneratedColumn<DateTime> get cachedAt =>
      $composableBuilder(column: $table.cachedAt, builder: (column) => column);
}

class $$LookaheadCacheTableTableManager extends RootTableManager<
    _$SunflowerDatabase,
    $LookaheadCacheTable,
    LookaheadCacheData,
    $$LookaheadCacheTableFilterComposer,
    $$LookaheadCacheTableOrderingComposer,
    $$LookaheadCacheTableAnnotationComposer,
    $$LookaheadCacheTableCreateCompanionBuilder,
    $$LookaheadCacheTableUpdateCompanionBuilder,
    (
      LookaheadCacheData,
      BaseReferences<_$SunflowerDatabase, $LookaheadCacheTable,
          LookaheadCacheData>
    ),
    LookaheadCacheData,
    PrefetchHooks Function()> {
  $$LookaheadCacheTableTableManager(
      _$SunflowerDatabase db, $LookaheadCacheTable table)
      : super(TableManagerState(
          db: db,
          table: table,
          createFilteringComposer: () =>
              $$LookaheadCacheTableFilterComposer($db: db, $table: table),
          createOrderingComposer: () =>
              $$LookaheadCacheTableOrderingComposer($db: db, $table: table),
          createComputedFieldComposer: () =>
              $$LookaheadCacheTableAnnotationComposer($db: db, $table: table),
          updateCompanionCallback: ({
            Value<String> queueId = const Value.absent(),
            Value<int> position = const Value.absent(),
            Value<String> mediaId = const Value.absent(),
            Value<String> title = const Value.absent(),
            Value<String> artistsJson = const Value.absent(),
            Value<int> durationMs = const Value.absent(),
            Value<String> source = const Value.absent(),
            Value<String?> streamUrl = const Value.absent(),
            Value<DateTime?> streamExpiresAt = const Value.absent(),
            Value<String?> mimeType = const Value.absent(),
            Value<DateTime> cachedAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              LookaheadCacheCompanion(
            queueId: queueId,
            position: position,
            mediaId: mediaId,
            title: title,
            artistsJson: artistsJson,
            durationMs: durationMs,
            source: source,
            streamUrl: streamUrl,
            streamExpiresAt: streamExpiresAt,
            mimeType: mimeType,
            cachedAt: cachedAt,
            rowid: rowid,
          ),
          createCompanionCallback: ({
            required String queueId,
            required int position,
            required String mediaId,
            Value<String> title = const Value.absent(),
            Value<String> artistsJson = const Value.absent(),
            Value<int> durationMs = const Value.absent(),
            Value<String> source = const Value.absent(),
            Value<String?> streamUrl = const Value.absent(),
            Value<DateTime?> streamExpiresAt = const Value.absent(),
            Value<String?> mimeType = const Value.absent(),
            Value<DateTime> cachedAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              LookaheadCacheCompanion.insert(
            queueId: queueId,
            position: position,
            mediaId: mediaId,
            title: title,
            artistsJson: artistsJson,
            durationMs: durationMs,
            source: source,
            streamUrl: streamUrl,
            streamExpiresAt: streamExpiresAt,
            mimeType: mimeType,
            cachedAt: cachedAt,
            rowid: rowid,
          ),
          withReferenceMapper: (p0) => p0
              .map((e) => (e.readTable(table), BaseReferences(db, table, e)))
              .toList(),
          prefetchHooksCallback: null,
        ));
}

typedef $$LookaheadCacheTableProcessedTableManager = ProcessedTableManager<
    _$SunflowerDatabase,
    $LookaheadCacheTable,
    LookaheadCacheData,
    $$LookaheadCacheTableFilterComposer,
    $$LookaheadCacheTableOrderingComposer,
    $$LookaheadCacheTableAnnotationComposer,
    $$LookaheadCacheTableCreateCompanionBuilder,
    $$LookaheadCacheTableUpdateCompanionBuilder,
    (
      LookaheadCacheData,
      BaseReferences<_$SunflowerDatabase, $LookaheadCacheTable,
          LookaheadCacheData>
    ),
    LookaheadCacheData,
    PrefetchHooks Function()>;
typedef $$RecentPlaysTableCreateCompanionBuilder = RecentPlaysCompanion
    Function({
  required String mediaId,
  Value<String> title,
  Value<String> artistName,
  Value<String> source,
  Value<String?> streamUrl,
  Value<int> durationMs,
  Value<int> playCount,
  Value<DateTime> lastPlayedAt,
  Value<int> rowid,
});
typedef $$RecentPlaysTableUpdateCompanionBuilder = RecentPlaysCompanion
    Function({
  Value<String> mediaId,
  Value<String> title,
  Value<String> artistName,
  Value<String> source,
  Value<String?> streamUrl,
  Value<int> durationMs,
  Value<int> playCount,
  Value<DateTime> lastPlayedAt,
  Value<int> rowid,
});

class $$RecentPlaysTableFilterComposer
    extends Composer<_$SunflowerDatabase, $RecentPlaysTable> {
  $$RecentPlaysTableFilterComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnFilters<String> get mediaId => $composableBuilder(
      column: $table.mediaId, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get title => $composableBuilder(
      column: $table.title, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get artistName => $composableBuilder(
      column: $table.artistName, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get source => $composableBuilder(
      column: $table.source, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get streamUrl => $composableBuilder(
      column: $table.streamUrl, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get durationMs => $composableBuilder(
      column: $table.durationMs, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get playCount => $composableBuilder(
      column: $table.playCount, builder: (column) => ColumnFilters(column));

  ColumnFilters<DateTime> get lastPlayedAt => $composableBuilder(
      column: $table.lastPlayedAt, builder: (column) => ColumnFilters(column));
}

class $$RecentPlaysTableOrderingComposer
    extends Composer<_$SunflowerDatabase, $RecentPlaysTable> {
  $$RecentPlaysTableOrderingComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnOrderings<String> get mediaId => $composableBuilder(
      column: $table.mediaId, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get title => $composableBuilder(
      column: $table.title, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get artistName => $composableBuilder(
      column: $table.artistName, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get source => $composableBuilder(
      column: $table.source, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get streamUrl => $composableBuilder(
      column: $table.streamUrl, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get durationMs => $composableBuilder(
      column: $table.durationMs, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get playCount => $composableBuilder(
      column: $table.playCount, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<DateTime> get lastPlayedAt => $composableBuilder(
      column: $table.lastPlayedAt,
      builder: (column) => ColumnOrderings(column));
}

class $$RecentPlaysTableAnnotationComposer
    extends Composer<_$SunflowerDatabase, $RecentPlaysTable> {
  $$RecentPlaysTableAnnotationComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  GeneratedColumn<String> get mediaId =>
      $composableBuilder(column: $table.mediaId, builder: (column) => column);

  GeneratedColumn<String> get title =>
      $composableBuilder(column: $table.title, builder: (column) => column);

  GeneratedColumn<String> get artistName => $composableBuilder(
      column: $table.artistName, builder: (column) => column);

  GeneratedColumn<String> get source =>
      $composableBuilder(column: $table.source, builder: (column) => column);

  GeneratedColumn<String> get streamUrl =>
      $composableBuilder(column: $table.streamUrl, builder: (column) => column);

  GeneratedColumn<int> get durationMs => $composableBuilder(
      column: $table.durationMs, builder: (column) => column);

  GeneratedColumn<int> get playCount =>
      $composableBuilder(column: $table.playCount, builder: (column) => column);

  GeneratedColumn<DateTime> get lastPlayedAt => $composableBuilder(
      column: $table.lastPlayedAt, builder: (column) => column);
}

class $$RecentPlaysTableTableManager extends RootTableManager<
    _$SunflowerDatabase,
    $RecentPlaysTable,
    RecentPlay,
    $$RecentPlaysTableFilterComposer,
    $$RecentPlaysTableOrderingComposer,
    $$RecentPlaysTableAnnotationComposer,
    $$RecentPlaysTableCreateCompanionBuilder,
    $$RecentPlaysTableUpdateCompanionBuilder,
    (
      RecentPlay,
      BaseReferences<_$SunflowerDatabase, $RecentPlaysTable, RecentPlay>
    ),
    RecentPlay,
    PrefetchHooks Function()> {
  $$RecentPlaysTableTableManager(
      _$SunflowerDatabase db, $RecentPlaysTable table)
      : super(TableManagerState(
          db: db,
          table: table,
          createFilteringComposer: () =>
              $$RecentPlaysTableFilterComposer($db: db, $table: table),
          createOrderingComposer: () =>
              $$RecentPlaysTableOrderingComposer($db: db, $table: table),
          createComputedFieldComposer: () =>
              $$RecentPlaysTableAnnotationComposer($db: db, $table: table),
          updateCompanionCallback: ({
            Value<String> mediaId = const Value.absent(),
            Value<String> title = const Value.absent(),
            Value<String> artistName = const Value.absent(),
            Value<String> source = const Value.absent(),
            Value<String?> streamUrl = const Value.absent(),
            Value<int> durationMs = const Value.absent(),
            Value<int> playCount = const Value.absent(),
            Value<DateTime> lastPlayedAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              RecentPlaysCompanion(
            mediaId: mediaId,
            title: title,
            artistName: artistName,
            source: source,
            streamUrl: streamUrl,
            durationMs: durationMs,
            playCount: playCount,
            lastPlayedAt: lastPlayedAt,
            rowid: rowid,
          ),
          createCompanionCallback: ({
            required String mediaId,
            Value<String> title = const Value.absent(),
            Value<String> artistName = const Value.absent(),
            Value<String> source = const Value.absent(),
            Value<String?> streamUrl = const Value.absent(),
            Value<int> durationMs = const Value.absent(),
            Value<int> playCount = const Value.absent(),
            Value<DateTime> lastPlayedAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              RecentPlaysCompanion.insert(
            mediaId: mediaId,
            title: title,
            artistName: artistName,
            source: source,
            streamUrl: streamUrl,
            durationMs: durationMs,
            playCount: playCount,
            lastPlayedAt: lastPlayedAt,
            rowid: rowid,
          ),
          withReferenceMapper: (p0) => p0
              .map((e) => (e.readTable(table), BaseReferences(db, table, e)))
              .toList(),
          prefetchHooksCallback: null,
        ));
}

typedef $$RecentPlaysTableProcessedTableManager = ProcessedTableManager<
    _$SunflowerDatabase,
    $RecentPlaysTable,
    RecentPlay,
    $$RecentPlaysTableFilterComposer,
    $$RecentPlaysTableOrderingComposer,
    $$RecentPlaysTableAnnotationComposer,
    $$RecentPlaysTableCreateCompanionBuilder,
    $$RecentPlaysTableUpdateCompanionBuilder,
    (
      RecentPlay,
      BaseReferences<_$SunflowerDatabase, $RecentPlaysTable, RecentPlay>
    ),
    RecentPlay,
    PrefetchHooks Function()>;
typedef $$HomeCacheTableCreateCompanionBuilder = HomeCacheCompanion Function({
  required String cacheKey,
  required String payloadJson,
  Value<DateTime> cachedAt,
  Value<int> rowid,
});
typedef $$HomeCacheTableUpdateCompanionBuilder = HomeCacheCompanion Function({
  Value<String> cacheKey,
  Value<String> payloadJson,
  Value<DateTime> cachedAt,
  Value<int> rowid,
});

class $$HomeCacheTableFilterComposer
    extends Composer<_$SunflowerDatabase, $HomeCacheTable> {
  $$HomeCacheTableFilterComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnFilters<String> get cacheKey => $composableBuilder(
      column: $table.cacheKey, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get payloadJson => $composableBuilder(
      column: $table.payloadJson, builder: (column) => ColumnFilters(column));

  ColumnFilters<DateTime> get cachedAt => $composableBuilder(
      column: $table.cachedAt, builder: (column) => ColumnFilters(column));
}

class $$HomeCacheTableOrderingComposer
    extends Composer<_$SunflowerDatabase, $HomeCacheTable> {
  $$HomeCacheTableOrderingComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnOrderings<String> get cacheKey => $composableBuilder(
      column: $table.cacheKey, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get payloadJson => $composableBuilder(
      column: $table.payloadJson, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<DateTime> get cachedAt => $composableBuilder(
      column: $table.cachedAt, builder: (column) => ColumnOrderings(column));
}

class $$HomeCacheTableAnnotationComposer
    extends Composer<_$SunflowerDatabase, $HomeCacheTable> {
  $$HomeCacheTableAnnotationComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  GeneratedColumn<String> get cacheKey =>
      $composableBuilder(column: $table.cacheKey, builder: (column) => column);

  GeneratedColumn<String> get payloadJson => $composableBuilder(
      column: $table.payloadJson, builder: (column) => column);

  GeneratedColumn<DateTime> get cachedAt =>
      $composableBuilder(column: $table.cachedAt, builder: (column) => column);
}

class $$HomeCacheTableTableManager extends RootTableManager<
    _$SunflowerDatabase,
    $HomeCacheTable,
    HomeCacheData,
    $$HomeCacheTableFilterComposer,
    $$HomeCacheTableOrderingComposer,
    $$HomeCacheTableAnnotationComposer,
    $$HomeCacheTableCreateCompanionBuilder,
    $$HomeCacheTableUpdateCompanionBuilder,
    (
      HomeCacheData,
      BaseReferences<_$SunflowerDatabase, $HomeCacheTable, HomeCacheData>
    ),
    HomeCacheData,
    PrefetchHooks Function()> {
  $$HomeCacheTableTableManager(_$SunflowerDatabase db, $HomeCacheTable table)
      : super(TableManagerState(
          db: db,
          table: table,
          createFilteringComposer: () =>
              $$HomeCacheTableFilterComposer($db: db, $table: table),
          createOrderingComposer: () =>
              $$HomeCacheTableOrderingComposer($db: db, $table: table),
          createComputedFieldComposer: () =>
              $$HomeCacheTableAnnotationComposer($db: db, $table: table),
          updateCompanionCallback: ({
            Value<String> cacheKey = const Value.absent(),
            Value<String> payloadJson = const Value.absent(),
            Value<DateTime> cachedAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              HomeCacheCompanion(
            cacheKey: cacheKey,
            payloadJson: payloadJson,
            cachedAt: cachedAt,
            rowid: rowid,
          ),
          createCompanionCallback: ({
            required String cacheKey,
            required String payloadJson,
            Value<DateTime> cachedAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              HomeCacheCompanion.insert(
            cacheKey: cacheKey,
            payloadJson: payloadJson,
            cachedAt: cachedAt,
            rowid: rowid,
          ),
          withReferenceMapper: (p0) => p0
              .map((e) => (e.readTable(table), BaseReferences(db, table, e)))
              .toList(),
          prefetchHooksCallback: null,
        ));
}

typedef $$HomeCacheTableProcessedTableManager = ProcessedTableManager<
    _$SunflowerDatabase,
    $HomeCacheTable,
    HomeCacheData,
    $$HomeCacheTableFilterComposer,
    $$HomeCacheTableOrderingComposer,
    $$HomeCacheTableAnnotationComposer,
    $$HomeCacheTableCreateCompanionBuilder,
    $$HomeCacheTableUpdateCompanionBuilder,
    (
      HomeCacheData,
      BaseReferences<_$SunflowerDatabase, $HomeCacheTable, HomeCacheData>
    ),
    HomeCacheData,
    PrefetchHooks Function()>;
typedef $$DownloadJobsTableCreateCompanionBuilder = DownloadJobsCompanion
    Function({
  required String mediaId,
  Value<String> title,
  required String sourceUrl,
  Value<String> status,
  Value<int> totalBytes,
  Value<int> receivedBytes,
  Value<String?> playlistId,
  Value<String?> error,
  Value<DateTime> updatedAt,
  Value<int> rowid,
});
typedef $$DownloadJobsTableUpdateCompanionBuilder = DownloadJobsCompanion
    Function({
  Value<String> mediaId,
  Value<String> title,
  Value<String> sourceUrl,
  Value<String> status,
  Value<int> totalBytes,
  Value<int> receivedBytes,
  Value<String?> playlistId,
  Value<String?> error,
  Value<DateTime> updatedAt,
  Value<int> rowid,
});

class $$DownloadJobsTableFilterComposer
    extends Composer<_$SunflowerDatabase, $DownloadJobsTable> {
  $$DownloadJobsTableFilterComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnFilters<String> get mediaId => $composableBuilder(
      column: $table.mediaId, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get title => $composableBuilder(
      column: $table.title, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get sourceUrl => $composableBuilder(
      column: $table.sourceUrl, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get status => $composableBuilder(
      column: $table.status, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get totalBytes => $composableBuilder(
      column: $table.totalBytes, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get receivedBytes => $composableBuilder(
      column: $table.receivedBytes, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get playlistId => $composableBuilder(
      column: $table.playlistId, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get error => $composableBuilder(
      column: $table.error, builder: (column) => ColumnFilters(column));

  ColumnFilters<DateTime> get updatedAt => $composableBuilder(
      column: $table.updatedAt, builder: (column) => ColumnFilters(column));
}

class $$DownloadJobsTableOrderingComposer
    extends Composer<_$SunflowerDatabase, $DownloadJobsTable> {
  $$DownloadJobsTableOrderingComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnOrderings<String> get mediaId => $composableBuilder(
      column: $table.mediaId, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get title => $composableBuilder(
      column: $table.title, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get sourceUrl => $composableBuilder(
      column: $table.sourceUrl, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get status => $composableBuilder(
      column: $table.status, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get totalBytes => $composableBuilder(
      column: $table.totalBytes, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get receivedBytes => $composableBuilder(
      column: $table.receivedBytes,
      builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get playlistId => $composableBuilder(
      column: $table.playlistId, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get error => $composableBuilder(
      column: $table.error, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<DateTime> get updatedAt => $composableBuilder(
      column: $table.updatedAt, builder: (column) => ColumnOrderings(column));
}

class $$DownloadJobsTableAnnotationComposer
    extends Composer<_$SunflowerDatabase, $DownloadJobsTable> {
  $$DownloadJobsTableAnnotationComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  GeneratedColumn<String> get mediaId =>
      $composableBuilder(column: $table.mediaId, builder: (column) => column);

  GeneratedColumn<String> get title =>
      $composableBuilder(column: $table.title, builder: (column) => column);

  GeneratedColumn<String> get sourceUrl =>
      $composableBuilder(column: $table.sourceUrl, builder: (column) => column);

  GeneratedColumn<String> get status =>
      $composableBuilder(column: $table.status, builder: (column) => column);

  GeneratedColumn<int> get totalBytes => $composableBuilder(
      column: $table.totalBytes, builder: (column) => column);

  GeneratedColumn<int> get receivedBytes => $composableBuilder(
      column: $table.receivedBytes, builder: (column) => column);

  GeneratedColumn<String> get playlistId => $composableBuilder(
      column: $table.playlistId, builder: (column) => column);

  GeneratedColumn<String> get error =>
      $composableBuilder(column: $table.error, builder: (column) => column);

  GeneratedColumn<DateTime> get updatedAt =>
      $composableBuilder(column: $table.updatedAt, builder: (column) => column);
}

class $$DownloadJobsTableTableManager extends RootTableManager<
    _$SunflowerDatabase,
    $DownloadJobsTable,
    DownloadJob,
    $$DownloadJobsTableFilterComposer,
    $$DownloadJobsTableOrderingComposer,
    $$DownloadJobsTableAnnotationComposer,
    $$DownloadJobsTableCreateCompanionBuilder,
    $$DownloadJobsTableUpdateCompanionBuilder,
    (
      DownloadJob,
      BaseReferences<_$SunflowerDatabase, $DownloadJobsTable, DownloadJob>
    ),
    DownloadJob,
    PrefetchHooks Function()> {
  $$DownloadJobsTableTableManager(
      _$SunflowerDatabase db, $DownloadJobsTable table)
      : super(TableManagerState(
          db: db,
          table: table,
          createFilteringComposer: () =>
              $$DownloadJobsTableFilterComposer($db: db, $table: table),
          createOrderingComposer: () =>
              $$DownloadJobsTableOrderingComposer($db: db, $table: table),
          createComputedFieldComposer: () =>
              $$DownloadJobsTableAnnotationComposer($db: db, $table: table),
          updateCompanionCallback: ({
            Value<String> mediaId = const Value.absent(),
            Value<String> title = const Value.absent(),
            Value<String> sourceUrl = const Value.absent(),
            Value<String> status = const Value.absent(),
            Value<int> totalBytes = const Value.absent(),
            Value<int> receivedBytes = const Value.absent(),
            Value<String?> playlistId = const Value.absent(),
            Value<String?> error = const Value.absent(),
            Value<DateTime> updatedAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              DownloadJobsCompanion(
            mediaId: mediaId,
            title: title,
            sourceUrl: sourceUrl,
            status: status,
            totalBytes: totalBytes,
            receivedBytes: receivedBytes,
            playlistId: playlistId,
            error: error,
            updatedAt: updatedAt,
            rowid: rowid,
          ),
          createCompanionCallback: ({
            required String mediaId,
            Value<String> title = const Value.absent(),
            required String sourceUrl,
            Value<String> status = const Value.absent(),
            Value<int> totalBytes = const Value.absent(),
            Value<int> receivedBytes = const Value.absent(),
            Value<String?> playlistId = const Value.absent(),
            Value<String?> error = const Value.absent(),
            Value<DateTime> updatedAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              DownloadJobsCompanion.insert(
            mediaId: mediaId,
            title: title,
            sourceUrl: sourceUrl,
            status: status,
            totalBytes: totalBytes,
            receivedBytes: receivedBytes,
            playlistId: playlistId,
            error: error,
            updatedAt: updatedAt,
            rowid: rowid,
          ),
          withReferenceMapper: (p0) => p0
              .map((e) => (e.readTable(table), BaseReferences(db, table, e)))
              .toList(),
          prefetchHooksCallback: null,
        ));
}

typedef $$DownloadJobsTableProcessedTableManager = ProcessedTableManager<
    _$SunflowerDatabase,
    $DownloadJobsTable,
    DownloadJob,
    $$DownloadJobsTableFilterComposer,
    $$DownloadJobsTableOrderingComposer,
    $$DownloadJobsTableAnnotationComposer,
    $$DownloadJobsTableCreateCompanionBuilder,
    $$DownloadJobsTableUpdateCompanionBuilder,
    (
      DownloadJob,
      BaseReferences<_$SunflowerDatabase, $DownloadJobsTable, DownloadJob>
    ),
    DownloadJob,
    PrefetchHooks Function()>;
typedef $$DownloadedTracksTableCreateCompanionBuilder
    = DownloadedTracksCompanion Function({
  required String mediaId,
  required String localPath,
  Value<int> bytes,
  Value<String?> sha256,
  Value<DateTime> completedAt,
  Value<int> rowid,
});
typedef $$DownloadedTracksTableUpdateCompanionBuilder
    = DownloadedTracksCompanion Function({
  Value<String> mediaId,
  Value<String> localPath,
  Value<int> bytes,
  Value<String?> sha256,
  Value<DateTime> completedAt,
  Value<int> rowid,
});

class $$DownloadedTracksTableFilterComposer
    extends Composer<_$SunflowerDatabase, $DownloadedTracksTable> {
  $$DownloadedTracksTableFilterComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnFilters<String> get mediaId => $composableBuilder(
      column: $table.mediaId, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get localPath => $composableBuilder(
      column: $table.localPath, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get bytes => $composableBuilder(
      column: $table.bytes, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get sha256 => $composableBuilder(
      column: $table.sha256, builder: (column) => ColumnFilters(column));

  ColumnFilters<DateTime> get completedAt => $composableBuilder(
      column: $table.completedAt, builder: (column) => ColumnFilters(column));
}

class $$DownloadedTracksTableOrderingComposer
    extends Composer<_$SunflowerDatabase, $DownloadedTracksTable> {
  $$DownloadedTracksTableOrderingComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnOrderings<String> get mediaId => $composableBuilder(
      column: $table.mediaId, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get localPath => $composableBuilder(
      column: $table.localPath, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get bytes => $composableBuilder(
      column: $table.bytes, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get sha256 => $composableBuilder(
      column: $table.sha256, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<DateTime> get completedAt => $composableBuilder(
      column: $table.completedAt, builder: (column) => ColumnOrderings(column));
}

class $$DownloadedTracksTableAnnotationComposer
    extends Composer<_$SunflowerDatabase, $DownloadedTracksTable> {
  $$DownloadedTracksTableAnnotationComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  GeneratedColumn<String> get mediaId =>
      $composableBuilder(column: $table.mediaId, builder: (column) => column);

  GeneratedColumn<String> get localPath =>
      $composableBuilder(column: $table.localPath, builder: (column) => column);

  GeneratedColumn<int> get bytes =>
      $composableBuilder(column: $table.bytes, builder: (column) => column);

  GeneratedColumn<String> get sha256 =>
      $composableBuilder(column: $table.sha256, builder: (column) => column);

  GeneratedColumn<DateTime> get completedAt => $composableBuilder(
      column: $table.completedAt, builder: (column) => column);
}

class $$DownloadedTracksTableTableManager extends RootTableManager<
    _$SunflowerDatabase,
    $DownloadedTracksTable,
    DownloadedTrack,
    $$DownloadedTracksTableFilterComposer,
    $$DownloadedTracksTableOrderingComposer,
    $$DownloadedTracksTableAnnotationComposer,
    $$DownloadedTracksTableCreateCompanionBuilder,
    $$DownloadedTracksTableUpdateCompanionBuilder,
    (
      DownloadedTrack,
      BaseReferences<_$SunflowerDatabase, $DownloadedTracksTable,
          DownloadedTrack>
    ),
    DownloadedTrack,
    PrefetchHooks Function()> {
  $$DownloadedTracksTableTableManager(
      _$SunflowerDatabase db, $DownloadedTracksTable table)
      : super(TableManagerState(
          db: db,
          table: table,
          createFilteringComposer: () =>
              $$DownloadedTracksTableFilterComposer($db: db, $table: table),
          createOrderingComposer: () =>
              $$DownloadedTracksTableOrderingComposer($db: db, $table: table),
          createComputedFieldComposer: () =>
              $$DownloadedTracksTableAnnotationComposer($db: db, $table: table),
          updateCompanionCallback: ({
            Value<String> mediaId = const Value.absent(),
            Value<String> localPath = const Value.absent(),
            Value<int> bytes = const Value.absent(),
            Value<String?> sha256 = const Value.absent(),
            Value<DateTime> completedAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              DownloadedTracksCompanion(
            mediaId: mediaId,
            localPath: localPath,
            bytes: bytes,
            sha256: sha256,
            completedAt: completedAt,
            rowid: rowid,
          ),
          createCompanionCallback: ({
            required String mediaId,
            required String localPath,
            Value<int> bytes = const Value.absent(),
            Value<String?> sha256 = const Value.absent(),
            Value<DateTime> completedAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              DownloadedTracksCompanion.insert(
            mediaId: mediaId,
            localPath: localPath,
            bytes: bytes,
            sha256: sha256,
            completedAt: completedAt,
            rowid: rowid,
          ),
          withReferenceMapper: (p0) => p0
              .map((e) => (e.readTable(table), BaseReferences(db, table, e)))
              .toList(),
          prefetchHooksCallback: null,
        ));
}

typedef $$DownloadedTracksTableProcessedTableManager = ProcessedTableManager<
    _$SunflowerDatabase,
    $DownloadedTracksTable,
    DownloadedTrack,
    $$DownloadedTracksTableFilterComposer,
    $$DownloadedTracksTableOrderingComposer,
    $$DownloadedTracksTableAnnotationComposer,
    $$DownloadedTracksTableCreateCompanionBuilder,
    $$DownloadedTracksTableUpdateCompanionBuilder,
    (
      DownloadedTrack,
      BaseReferences<_$SunflowerDatabase, $DownloadedTracksTable,
          DownloadedTrack>
    ),
    DownloadedTrack,
    PrefetchHooks Function()>;
typedef $$PendingMutationsTableCreateCompanionBuilder
    = PendingMutationsCompanion Function({
  required String idempotencyKey,
  required String kind,
  required String method,
  required String path,
  Value<String> bodyJson,
  Value<String> status,
  Value<int> attempts,
  required int clientClock,
  Value<int> nextAttemptAt,
  Value<int> priority,
  Value<String?> error,
  Value<DateTime> createdAt,
  Value<int> rowid,
});
typedef $$PendingMutationsTableUpdateCompanionBuilder
    = PendingMutationsCompanion Function({
  Value<String> idempotencyKey,
  Value<String> kind,
  Value<String> method,
  Value<String> path,
  Value<String> bodyJson,
  Value<String> status,
  Value<int> attempts,
  Value<int> clientClock,
  Value<int> nextAttemptAt,
  Value<int> priority,
  Value<String?> error,
  Value<DateTime> createdAt,
  Value<int> rowid,
});

class $$PendingMutationsTableFilterComposer
    extends Composer<_$SunflowerDatabase, $PendingMutationsTable> {
  $$PendingMutationsTableFilterComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnFilters<String> get idempotencyKey => $composableBuilder(
      column: $table.idempotencyKey,
      builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get kind => $composableBuilder(
      column: $table.kind, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get method => $composableBuilder(
      column: $table.method, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get path => $composableBuilder(
      column: $table.path, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get bodyJson => $composableBuilder(
      column: $table.bodyJson, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get status => $composableBuilder(
      column: $table.status, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get attempts => $composableBuilder(
      column: $table.attempts, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get clientClock => $composableBuilder(
      column: $table.clientClock, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get nextAttemptAt => $composableBuilder(
      column: $table.nextAttemptAt, builder: (column) => ColumnFilters(column));

  ColumnFilters<int> get priority => $composableBuilder(
      column: $table.priority, builder: (column) => ColumnFilters(column));

  ColumnFilters<String> get error => $composableBuilder(
      column: $table.error, builder: (column) => ColumnFilters(column));

  ColumnFilters<DateTime> get createdAt => $composableBuilder(
      column: $table.createdAt, builder: (column) => ColumnFilters(column));
}

class $$PendingMutationsTableOrderingComposer
    extends Composer<_$SunflowerDatabase, $PendingMutationsTable> {
  $$PendingMutationsTableOrderingComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  ColumnOrderings<String> get idempotencyKey => $composableBuilder(
      column: $table.idempotencyKey,
      builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get kind => $composableBuilder(
      column: $table.kind, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get method => $composableBuilder(
      column: $table.method, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get path => $composableBuilder(
      column: $table.path, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get bodyJson => $composableBuilder(
      column: $table.bodyJson, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get status => $composableBuilder(
      column: $table.status, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get attempts => $composableBuilder(
      column: $table.attempts, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get clientClock => $composableBuilder(
      column: $table.clientClock, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get nextAttemptAt => $composableBuilder(
      column: $table.nextAttemptAt,
      builder: (column) => ColumnOrderings(column));

  ColumnOrderings<int> get priority => $composableBuilder(
      column: $table.priority, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<String> get error => $composableBuilder(
      column: $table.error, builder: (column) => ColumnOrderings(column));

  ColumnOrderings<DateTime> get createdAt => $composableBuilder(
      column: $table.createdAt, builder: (column) => ColumnOrderings(column));
}

class $$PendingMutationsTableAnnotationComposer
    extends Composer<_$SunflowerDatabase, $PendingMutationsTable> {
  $$PendingMutationsTableAnnotationComposer({
    required super.$db,
    required super.$table,
    super.joinBuilder,
    super.$addJoinBuilderToRootComposer,
    super.$removeJoinBuilderFromRootComposer,
  });
  GeneratedColumn<String> get idempotencyKey => $composableBuilder(
      column: $table.idempotencyKey, builder: (column) => column);

  GeneratedColumn<String> get kind =>
      $composableBuilder(column: $table.kind, builder: (column) => column);

  GeneratedColumn<String> get method =>
      $composableBuilder(column: $table.method, builder: (column) => column);

  GeneratedColumn<String> get path =>
      $composableBuilder(column: $table.path, builder: (column) => column);

  GeneratedColumn<String> get bodyJson =>
      $composableBuilder(column: $table.bodyJson, builder: (column) => column);

  GeneratedColumn<String> get status =>
      $composableBuilder(column: $table.status, builder: (column) => column);

  GeneratedColumn<int> get attempts =>
      $composableBuilder(column: $table.attempts, builder: (column) => column);

  GeneratedColumn<int> get clientClock => $composableBuilder(
      column: $table.clientClock, builder: (column) => column);

  GeneratedColumn<int> get nextAttemptAt => $composableBuilder(
      column: $table.nextAttemptAt, builder: (column) => column);

  GeneratedColumn<int> get priority =>
      $composableBuilder(column: $table.priority, builder: (column) => column);

  GeneratedColumn<String> get error =>
      $composableBuilder(column: $table.error, builder: (column) => column);

  GeneratedColumn<DateTime> get createdAt =>
      $composableBuilder(column: $table.createdAt, builder: (column) => column);
}

class $$PendingMutationsTableTableManager extends RootTableManager<
    _$SunflowerDatabase,
    $PendingMutationsTable,
    PendingMutation,
    $$PendingMutationsTableFilterComposer,
    $$PendingMutationsTableOrderingComposer,
    $$PendingMutationsTableAnnotationComposer,
    $$PendingMutationsTableCreateCompanionBuilder,
    $$PendingMutationsTableUpdateCompanionBuilder,
    (
      PendingMutation,
      BaseReferences<_$SunflowerDatabase, $PendingMutationsTable,
          PendingMutation>
    ),
    PendingMutation,
    PrefetchHooks Function()> {
  $$PendingMutationsTableTableManager(
      _$SunflowerDatabase db, $PendingMutationsTable table)
      : super(TableManagerState(
          db: db,
          table: table,
          createFilteringComposer: () =>
              $$PendingMutationsTableFilterComposer($db: db, $table: table),
          createOrderingComposer: () =>
              $$PendingMutationsTableOrderingComposer($db: db, $table: table),
          createComputedFieldComposer: () =>
              $$PendingMutationsTableAnnotationComposer($db: db, $table: table),
          updateCompanionCallback: ({
            Value<String> idempotencyKey = const Value.absent(),
            Value<String> kind = const Value.absent(),
            Value<String> method = const Value.absent(),
            Value<String> path = const Value.absent(),
            Value<String> bodyJson = const Value.absent(),
            Value<String> status = const Value.absent(),
            Value<int> attempts = const Value.absent(),
            Value<int> clientClock = const Value.absent(),
            Value<int> nextAttemptAt = const Value.absent(),
            Value<int> priority = const Value.absent(),
            Value<String?> error = const Value.absent(),
            Value<DateTime> createdAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              PendingMutationsCompanion(
            idempotencyKey: idempotencyKey,
            kind: kind,
            method: method,
            path: path,
            bodyJson: bodyJson,
            status: status,
            attempts: attempts,
            clientClock: clientClock,
            nextAttemptAt: nextAttemptAt,
            priority: priority,
            error: error,
            createdAt: createdAt,
            rowid: rowid,
          ),
          createCompanionCallback: ({
            required String idempotencyKey,
            required String kind,
            required String method,
            required String path,
            Value<String> bodyJson = const Value.absent(),
            Value<String> status = const Value.absent(),
            Value<int> attempts = const Value.absent(),
            required int clientClock,
            Value<int> nextAttemptAt = const Value.absent(),
            Value<int> priority = const Value.absent(),
            Value<String?> error = const Value.absent(),
            Value<DateTime> createdAt = const Value.absent(),
            Value<int> rowid = const Value.absent(),
          }) =>
              PendingMutationsCompanion.insert(
            idempotencyKey: idempotencyKey,
            kind: kind,
            method: method,
            path: path,
            bodyJson: bodyJson,
            status: status,
            attempts: attempts,
            clientClock: clientClock,
            nextAttemptAt: nextAttemptAt,
            priority: priority,
            error: error,
            createdAt: createdAt,
            rowid: rowid,
          ),
          withReferenceMapper: (p0) => p0
              .map((e) => (e.readTable(table), BaseReferences(db, table, e)))
              .toList(),
          prefetchHooksCallback: null,
        ));
}

typedef $$PendingMutationsTableProcessedTableManager = ProcessedTableManager<
    _$SunflowerDatabase,
    $PendingMutationsTable,
    PendingMutation,
    $$PendingMutationsTableFilterComposer,
    $$PendingMutationsTableOrderingComposer,
    $$PendingMutationsTableAnnotationComposer,
    $$PendingMutationsTableCreateCompanionBuilder,
    $$PendingMutationsTableUpdateCompanionBuilder,
    (
      PendingMutation,
      BaseReferences<_$SunflowerDatabase, $PendingMutationsTable,
          PendingMutation>
    ),
    PendingMutation,
    PrefetchHooks Function()>;

class $SunflowerDatabaseManager {
  final _$SunflowerDatabase _db;
  $SunflowerDatabaseManager(this._db);
  $$LookaheadCacheTableTableManager get lookaheadCache =>
      $$LookaheadCacheTableTableManager(_db, _db.lookaheadCache);
  $$RecentPlaysTableTableManager get recentPlays =>
      $$RecentPlaysTableTableManager(_db, _db.recentPlays);
  $$HomeCacheTableTableManager get homeCache =>
      $$HomeCacheTableTableManager(_db, _db.homeCache);
  $$DownloadJobsTableTableManager get downloadJobs =>
      $$DownloadJobsTableTableManager(_db, _db.downloadJobs);
  $$DownloadedTracksTableTableManager get downloadedTracks =>
      $$DownloadedTracksTableTableManager(_db, _db.downloadedTracks);
  $$PendingMutationsTableTableManager get pendingMutations =>
      $$PendingMutationsTableTableManager(_db, _db.pendingMutations);
}
