# Known Issues

## WeChat / QQ / Feishu Text Replacement

### Description

Replacing selected text in some native desktop applications is not always reliable.

Examples:

* WeChat
* QQ
* Feishu
* DingTalk

### Current Behavior

In some cases:

1. User selects text
2. Opens ClipLite
3. Selects clipboard item
4. Text is inserted instead of replacing the selected text

### Investigation

Browser applications:

* Chrome
* Edge
* Firefox

currently work correctly.

Native desktop applications may lose selection state when focus changes.

### Notes

Ditto exhibits similar limitations in some scenarios.

### Status

Known limitation.

Not a current development priority.

## Transparency

Transparency setting exists but visual effect is not fully applied.

### Status

Planned fix.

## History Cleanup

Cleanup modes are not fully implemented.

### Planned

Support:

* By Time
* By Count
* Never Auto Clean
