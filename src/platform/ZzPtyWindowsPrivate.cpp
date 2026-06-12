/**
 * @file ZzPtyWindowsPrivate.cpp
 * @brief ZzPtyWindows 私有实现 (ConPTY)。
 */

#include "platform/ZzPtyWindowsPrivate.h"

#if defined(ZZ_PLATFORM_WINDOWS)

#include <QFile>

#include <vector>

namespace ZzPlatform {

void ZzPtyReaderThread::run()
{
    char buffer[8192];
    DWORD bytesRead = 0;
    for (;;) {
        const BOOL ok = ::ReadFile(m_outputRead, buffer, sizeof(buffer), &bytesRead, nullptr);
        if (!ok || bytesRead == 0) {
            break;  // 管道关闭或出错。
        }
        {
            QFile f(QStringLiteral("C:/Users/guomaojie/AppData/Local/Temp/zzout.txt"));
            if (f.open(QIODevice::Append)) {
                f.write(buffer, static_cast<qint64>(bytesRead));
            }
        }
        emit chunkRead(QByteArray(buffer, static_cast<int>(bytesRead)));
    }
}

ZzPtyWindowsPrivate::~ZzPtyWindowsPrivate()
{
    terminate();
}

bool ZzPtyWindowsPrivate::spawn(const QString& program, const QStringList& arguments,
                                const QSize& size, QString& errorMessage)
{
    // [诊断] GUI 进程默认无控制台; 尝试分配一个隐藏控制台后再创建 ConPTY。
    if (::GetConsoleWindow() == nullptr) {
        ::AllocConsole();
        ::ShowWindow(::GetConsoleWindow(), SW_HIDE);
    }

    HANDLE inputRead = INVALID_HANDLE_VALUE;
    HANDLE outputWrite = INVALID_HANDLE_VALUE;

    if (!::CreatePipe(&inputRead, &inputWrite, nullptr, 0)) {
        errorMessage = QStringLiteral("CreatePipe(input) 失败");
        return false;
    }
    if (!::CreatePipe(&outputRead, &outputWrite, nullptr, 0)) {
        errorMessage = QStringLiteral("CreatePipe(output) 失败");
        ::CloseHandle(inputRead);
        return false;
    }

    const COORD consoleSize{static_cast<SHORT>(size.width()), static_cast<SHORT>(size.height())};
    const HRESULT hr = ::CreatePseudoConsole(consoleSize, inputRead, outputWrite, 0, &pseudoConsole);

    if (FAILED(hr)) {
        ::CloseHandle(inputRead);
        ::CloseHandle(outputWrite);
        errorMessage = QStringLiteral("CreatePseudoConsole 失败 (需要 Windows 10 1809+)");
        return false;
    }

    if (!prepareStartupInfo(errorMessage)) {
        ::CloseHandle(inputRead);
        ::CloseHandle(outputWrite);
        return false;
    }

    // 构造命令行 (可写缓冲区)。
    QString cmdLine = QStringLiteral("\"%1\"").arg(program);
    for (const QString& arg : arguments) {
        cmdLine += QStringLiteral(" \"%1\"").arg(arg);
    }
    std::vector<wchar_t> cmdBuffer(cmdLine.size() + 1);
    cmdLine.toWCharArray(cmdBuffer.data());
    cmdBuffer[cmdLine.size()] = L'\0';

    const BOOL created = ::CreateProcessW(
        nullptr, cmdBuffer.data(), nullptr, nullptr, FALSE,
        EXTENDED_STARTUPINFO_PRESENT, nullptr, nullptr,
        &startupInfo.StartupInfo, &processInfo);

    // conhost 已持有 PTY 端句柄副本, 在 CreateProcess 之后再关闭本地副本。
    ::CloseHandle(inputRead);
    ::CloseHandle(outputWrite);

    if (!created) {
        errorMessage = QStringLiteral("CreateProcessW 失败, 错误码 %1").arg(::GetLastError());
        return false;
    }

    running = true;
    return true;
}

bool ZzPtyWindowsPrivate::prepareStartupInfo(QString& errorMessage)
{
    startupInfo.StartupInfo.cb = sizeof(STARTUPINFOEXW);

    SIZE_T attrListSize = 0;
    ::InitializeProcThreadAttributeList(nullptr, 1, 0, &attrListSize);
    startupInfo.lpAttributeList =
        reinterpret_cast<LPPROC_THREAD_ATTRIBUTE_LIST>(::HeapAlloc(::GetProcessHeap(), 0, attrListSize));
    if (!startupInfo.lpAttributeList) {
        errorMessage = QStringLiteral("分配属性列表失败");
        return false;
    }

    if (!::InitializeProcThreadAttributeList(startupInfo.lpAttributeList, 1, 0, &attrListSize)) {
        errorMessage = QStringLiteral("InitializeProcThreadAttributeList 失败");
        return false;
    }

    if (!::UpdateProcThreadAttribute(startupInfo.lpAttributeList, 0,
                                     PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, pseudoConsole,
                                     sizeof(pseudoConsole), nullptr, nullptr)) {
        errorMessage = QStringLiteral("UpdateProcThreadAttribute 失败");
        return false;
    }
    return true;
}

qint64 ZzPtyWindowsPrivate::writeData(const QByteArray& data)
{
    if (inputWrite == INVALID_HANDLE_VALUE) {
        return -1;
    }
    DWORD written = 0;
    const BOOL ok = ::WriteFile(inputWrite, data.constData(), static_cast<DWORD>(data.size()),
                                &written, nullptr);
    {
        QFile f(QStringLiteral("C:/Users/guomaojie/AppData/Local/Temp/zzkeylog.txt"));
        if (f.open(QIODevice::Append | QIODevice::Text)) {
            f.write("WRITEFILE ok=" + QByteArray::number(static_cast<int>(ok)) +
                    " written=" + QByteArray::number(static_cast<qulonglong>(written)) +
                    " data=" + data.toHex() + "\n");
        }
    }
    return ok ? static_cast<qint64>(written) : -1;
}

void ZzPtyWindowsPrivate::setWindowSize(const QSize& size)
{
    if (pseudoConsole) {
        const COORD c{static_cast<SHORT>(size.width()), static_cast<SHORT>(size.height())};
        ::ResizePseudoConsole(pseudoConsole, c);
    }
}

void ZzPtyWindowsPrivate::terminate()
{
    if (pseudoConsole) {
        ::ClosePseudoConsole(pseudoConsole);  // 触发子进程退出, 读取线程随之结束。
        pseudoConsole = nullptr;
    }
    if (reader) {
        reader->wait(2000);
        reader->deleteLater();
        reader = nullptr;
    }
    if (inputWrite != INVALID_HANDLE_VALUE) {
        ::CloseHandle(inputWrite);
        inputWrite = INVALID_HANDLE_VALUE;
    }
    if (outputRead != INVALID_HANDLE_VALUE) {
        ::CloseHandle(outputRead);
        outputRead = INVALID_HANDLE_VALUE;
    }
    if (processInfo.hThread) {
        ::CloseHandle(processInfo.hThread);
        processInfo.hThread = nullptr;
    }
    if (processInfo.hProcess) {
        ::CloseHandle(processInfo.hProcess);
        processInfo.hProcess = nullptr;
    }
    if (startupInfo.lpAttributeList) {
        ::DeleteProcThreadAttributeList(startupInfo.lpAttributeList);
        ::HeapFree(::GetProcessHeap(), 0, startupInfo.lpAttributeList);
        startupInfo.lpAttributeList = nullptr;
    }
    running = false;
}

}  // namespace ZzPlatform

#endif  // ZZ_PLATFORM_WINDOWS
