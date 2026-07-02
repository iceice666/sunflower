import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/api/sunflower_api.dart';
import 'package:sunflower/core/auth/token_store.dart';
import 'package:sunflower/features/settings/youtube_credentials_setting.dart';

void main() {
  testWidgets('saves pasted InnerTube token and clears the field',
      (tester) async {
    final api = _RecordingApi();

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          localModeProvider.overrideWith((ref) async => false),
          tokenProvider.overrideWith((ref) async => 'test-token'),
          serverUrlProvider.overrideWith((ref) async => 'http://test'),
          sunflowerApiProvider.overrideWithValue(api),
        ],
        child: const MaterialApp(
          home: Scaffold(body: YoutubeCredentialsSetting()),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.enterText(
      _textFieldWithLabel('InnerTube token'),
      'po_token=po\nvisitor_data=visitor',
    );
    await tester.pump();
    await tester.tap(find.widgetWithText(FilledButton, 'Save'));
    await tester.pumpAndSettle();

    expect(api.savedCookies, '');
    expect(api.savedToken, 'po_token=po\nvisitor_data=visitor');
    expect(find.text('YouTube credentials saved'), findsOneWidget);
    final tokenField = tester.widget<TextField>(
      _textFieldWithLabel('InnerTube token'),
    );
    expect(tokenField.controller?.text, '');
  });
}

Finder _textFieldWithLabel(String label) {
  return find.byWidgetPredicate(
    (widget) => widget is TextField && widget.decoration?.labelText == label,
  );
}

class _RecordingApi extends SunflowerApi {
  _RecordingApi() : super(baseUrl: 'http://test', token: 'test-token');

  String? savedCookies;
  String? savedToken;

  @override
  Future<YoutubeCredentialStatus> youtubeCredentialStatus() async {
    return const YoutubeCredentialStatus(status: 'unknown');
  }

  @override
  Future<void> uploadYoutubeCredentials({
    String cookies = '',
    String innertubeToken = '',
  }) async {
    savedCookies = cookies;
    savedToken = innertubeToken;
  }
}
