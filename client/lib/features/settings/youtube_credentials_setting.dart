import 'package:dio/dio.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import '../../core/auth/token_store.dart';

final youtubeCredentialStatusProvider =
    FutureProvider.autoDispose<YoutubeCredentialStatus>((ref) {
  return ref.watch(sunflowerApiProvider).youtubeCredentialStatus();
});

class YoutubeCredentialsSetting extends ConsumerStatefulWidget {
  const YoutubeCredentialsSetting({super.key});

  @override
  ConsumerState<YoutubeCredentialsSetting> createState() =>
      _YoutubeCredentialsSettingState();
}

class _YoutubeCredentialsSettingState
    extends ConsumerState<YoutubeCredentialsSetting> {
  final _cookiesController = TextEditingController();
  final _tokenController = TextEditingController();
  bool _saving = false;

  bool get _hasPayload =>
      _cookiesController.text.trim().isNotEmpty ||
      _tokenController.text.trim().isNotEmpty;

  @override
  void initState() {
    super.initState();
    _cookiesController.addListener(_onTextChanged);
    _tokenController.addListener(_onTextChanged);
  }

  @override
  void dispose() {
    _cookiesController
      ..removeListener(_onTextChanged)
      ..dispose();
    _tokenController
      ..removeListener(_onTextChanged)
      ..dispose();
    super.dispose();
  }

  void _onTextChanged() => setState(() {});

  @override
  Widget build(BuildContext context) {
    final localMode = ref.watch(localModeProvider);
    final token = ref.watch(tokenProvider);
    final serverUrl = ref.watch(serverUrlProvider);
    final loadingCredentials =
        localMode.isLoading || token.isLoading || serverUrl.isLoading;
    final unavailable = localMode.valueOrNull == true ||
        token.valueOrNull == null ||
        (serverUrl.valueOrNull?.isEmpty ?? true);
    final status = loadingCredentials || unavailable
        ? null
        : ref.watch(youtubeCredentialStatusProvider);
    final enabled = !loadingCredentials && !unavailable && !_saving;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ListTile(
          leading: const Icon(Icons.video_library_outlined),
          title: const Text('YouTube credentials'),
          subtitle: Text(_statusText(
            loadingCredentials: loadingCredentials,
            unavailable: unavailable,
            status: status,
          )),
          trailing: status?.isLoading == true
              ? const SizedBox.square(
                  dimension: 18,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : IconButton(
                  tooltip: 'Refresh',
                  icon: const Icon(Icons.refresh),
                  onPressed: enabled
                      ? () => ref.invalidate(youtubeCredentialStatusProvider)
                      : null,
                ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
          child: TextField(
            controller: _cookiesController,
            enabled: enabled,
            minLines: 3,
            maxLines: 6,
            keyboardType: TextInputType.multiline,
            textInputAction: TextInputAction.newline,
            autocorrect: false,
            enableSuggestions: false,
            decoration: const InputDecoration(
              labelText: 'Cookie export',
              alignLabelWithHint: true,
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
          child: TextField(
            controller: _tokenController,
            enabled: enabled,
            minLines: 2,
            maxLines: 4,
            keyboardType: TextInputType.multiline,
            textInputAction: TextInputAction.newline,
            autocorrect: false,
            enableSuggestions: false,
            decoration: const InputDecoration(
              labelText: 'InnerTube token',
              hintText: 'po_token=...\nvisitor_data=...',
              alignLabelWithHint: true,
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
          child: Align(
            alignment: Alignment.centerRight,
            child: FilledButton.icon(
              onPressed: enabled && _hasPayload ? _save : null,
              icon: _saving
                  ? const SizedBox.square(
                      dimension: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.upload),
              label: Text(_saving ? 'Saving' : 'Save'),
            ),
          ),
        ),
      ],
    );
  }

  String _statusText({
    required bool loadingCredentials,
    required bool unavailable,
    required AsyncValue<YoutubeCredentialStatus>? status,
  }) {
    if (loadingCredentials) return 'Loading server credentials';
    if (unavailable) return 'Pair with a server first';
    return status?.when(
          data: (value) {
            final detail = value.detail?.trim();
            if (detail != null && detail.isNotEmpty) {
              return '${value.status}: $detail';
            }
            return 'Status: ${value.status}';
          },
          error: (error, _) => _statusErrorText(error),
          loading: () => 'Checking status',
        ) ??
        'Checking status';
  }

  String _statusErrorText(Object error) {
    if (error is DioException &&
        error.response?.data is Map &&
        (error.response?.data as Map)['error'] == 'cookies_disabled') {
      return 'Server cookie encryption is not configured';
    }
    return 'Status unavailable';
  }

  Future<void> _save() async {
    FocusScope.of(context).unfocus();
    setState(() => _saving = true);
    try {
      await ref.read(sunflowerApiProvider).uploadYoutubeCredentials(
            cookies: _cookiesController.text,
            innertubeToken: _tokenController.text,
          );
      if (!mounted) return;
      _cookiesController.clear();
      _tokenController.clear();
      ref.invalidate(youtubeCredentialStatusProvider);
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('YouTube credentials saved')),
      );
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(_saveErrorText(error))),
      );
    } finally {
      if (mounted) {
        setState(() => _saving = false);
      }
    }
  }

  String _saveErrorText(Object error) {
    if (error is DioException &&
        error.response?.data is Map &&
        (error.response?.data as Map)['error'] == 'cookies_disabled') {
      return 'Set SUNFLOWER_COOKIE_KEY on the server first';
    }
    if (error is DioException &&
        error.response?.data is Map &&
        (error.response?.data as Map)['error'] == 'invalid_format') {
      return 'Paste a cookie export or InnerTube token';
    }
    return 'Could not save YouTube credentials';
  }
}
