import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/auth/register_device.dart';
import '../../core/auth/token_store.dart';

class ServerSetupScreen extends ConsumerStatefulWidget {
  const ServerSetupScreen({super.key});

  @override
  ConsumerState<ServerSetupScreen> createState() => _ServerSetupScreenState();
}

class _ServerSetupScreenState extends ConsumerState<ServerSetupScreen> {
  final _formKey = GlobalKey<FormState>();
  final _urlController = TextEditingController(
    text: 'http://192.168.1.1:8080',
  );
  final _pairingController = TextEditingController();
  bool _loading = false;
  bool _testing = false;
  String? _error;
  String? _status;

  @override
  void dispose() {
    _urlController.dispose();
    _pairingController.dispose();
    super.dispose();
  }

  String get _normalizedUrl =>
      _urlController.text.trim().replaceAll(RegExp(r'/$'), '');

  String? _validateUrl(String? v) {
    if (v == null || v.trim().isEmpty) {
      return 'Please enter a server URL';
    }
    final uri = Uri.tryParse(v.trim());
    if (uri == null || !uri.hasScheme) {
      return 'Enter a full URL (e.g. http://...)';
    }
    return null;
  }

  Future<void> _testConnection() async {
    final urlError = _validateUrl(_urlController.text);
    if (urlError != null) {
      setState(() => _error = urlError);
      return;
    }
    setState(() {
      _testing = true;
      _error = null;
      _status = null;
    });

    final url = _normalizedUrl;
    try {
      final status = await fetchSetupStatus(url);
      setState(() {
        _status = status.configured
            ? 'Server ready for pairing'
            : 'Owner setup is required in the admin dashboard';
      });
    } catch (e) {
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _testing = false);
    }
  }

  Future<void> _pair() async {
    if (!_formKey.currentState!.validate()) return;
    setState(() {
      _loading = true;
      _error = null;
      _status = null;
    });

    final url = _normalizedUrl;
    try {
      final result = await registerDevice(
        url,
        pairingCode: _pairingController.text.trim(),
      );
      await saveCredentials(ref, url, result.token, deviceId: result.deviceId);
      // tokenProvider invalidation triggers app re-route to SongsScreen.
    } catch (e) {
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Form(
            key: _formKey,
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Icon(
                  Icons.local_florist,
                  size: 52,
                  color: Theme.of(context).colorScheme.primary,
                ),
                const SizedBox(height: 12),
                const Text(
                  'Sunflower',
                  style: TextStyle(fontSize: 34, fontWeight: FontWeight.w900),
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 8),
                const Text(
                  'Pair this device from the admin dashboard',
                  style: TextStyle(fontSize: 16),
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 40),
                TextFormField(
                  controller: _urlController,
                  decoration: const InputDecoration(
                    labelText: 'Server URL',
                    hintText: 'http://192.168.1.x:8080',
                    border: OutlineInputBorder(),
                  ),
                  keyboardType: TextInputType.url,
                  autocorrect: false,
                  validator: _validateUrl,
                ),
                const SizedBox(height: 16),
                TextFormField(
                  controller: _pairingController,
                  decoration: const InputDecoration(
                    labelText: 'Pairing code',
                    hintText: '7K4D-91QF',
                    border: OutlineInputBorder(),
                  ),
                  textCapitalization: TextCapitalization.characters,
                  autocorrect: false,
                  validator: (v) {
                    if (v == null || v.trim().isEmpty) {
                      return 'Enter a pairing code';
                    }
                    return null;
                  },
                ),
                const SizedBox(height: 16),
                if (_status != null)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 16),
                    child: Text(
                      _status!,
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.primary,
                      ),
                    ),
                  ),
                if (_error != null)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 16),
                    child: Text(
                      _error!,
                      style:
                          TextStyle(color: Theme.of(context).colorScheme.error),
                    ),
                  ),
                Row(
                  children: [
                    Expanded(
                      child: OutlinedButton(
                        onPressed:
                            _testing || _loading ? null : _testConnection,
                        child: _testing
                            ? const SizedBox(
                                height: 20,
                                width: 20,
                                child:
                                    CircularProgressIndicator(strokeWidth: 2),
                              )
                            : const Text('Test connection'),
                      ),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: FilledButton(
                        onPressed: _loading || _testing ? null : _pair,
                        child: _loading
                            ? const SizedBox(
                                height: 20,
                                width: 20,
                                child:
                                    CircularProgressIndicator(strokeWidth: 2),
                              )
                            : const Text('Pair device'),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
