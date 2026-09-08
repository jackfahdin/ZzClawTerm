# Translation

NyaTerm includes multi-provider text translation for logs, errors, and command descriptions in the terminal.

## How to use it

1. Select text in the terminal
2. Right-click to open the context menu
3. Open the **Translate** submenu
4. Choose one of the available providers
5. Read the result in the popup dialog

The dialog shows:

- The original text
- The translated result
- The detected source language, when provided
- A one-click copy action for the translated text

## Which providers appear

Not every provider is always shown in the terminal context menu.

### Available out of the box

These providers do not require extra setup:

- **Google**
- **Microsoft**

### Shown after credentials are configured

These providers appear only after credentials are entered in **Settings → Translation**:

- **DeepL**
- **Baidu**
- **Alibaba**
- **Youdao**

## Translation settings

In **Settings → Translation**, you can configure:

- **Target language**
- **API credentials** for each provider

One important detail: the settings page mainly manages target language and credentials. It does **not** define one permanent default provider for all translation actions. The actual provider is still chosen from the terminal context menu when you translate.

## Good use cases

- Quickly understanding English errors or third-party logs
- Reading mixed-language output more efficiently
- Comparing translated operational notes or config comments
- Translating selected output before forwarding it to a teammate
