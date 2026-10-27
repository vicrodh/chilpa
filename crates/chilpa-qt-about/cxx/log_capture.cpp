// I preserve stderr while forwarding bounded complete lines to the facade.
// One reader blocks on the pipe. It never calls Qt or writes diagnostic errors.
#include <array>
#include <cerrno>
#include <cstdio>
#include <chrono>
#include <condition_variable>
#include <memory>
#include <mutex>
#include <string>

#include <fcntl.h>
#ifdef _WIN32
#include <io.h>
#include <process.h>
#include <windows.h>
#define CHILPA_DUP _dup
#define CHILPA_DUP2 _dup2
#define CHILPA_CLOSE _close
#define CHILPA_READ _read
#define CHILPA_WRITE _write
#else
#include <unistd.h>
#include <pthread.h>
#define CHILPA_DUP dup
#define CHILPA_DUP2 dup2
#define CHILPA_CLOSE close
#define CHILPA_READ read
#define CHILPA_WRITE write
#endif

extern "C" void chilpa_developer_log_line(const char *, std::size_t);
namespace {
std::mutex lifecycle;
#ifdef _WIN32
HANDLE reader = nullptr;
#else
pthread_t reader;
#endif
bool running = false;
struct ReaderState {
    int original = -1;
    int input = -1;
    std::mutex mutex;
    std::condition_variable finished;
    bool done = false;
};
std::shared_ptr<ReaderState> current;
bool installed = false;
constexpr std::size_t lineLimit = 4096;

void consume(const std::shared_ptr<ReaderState> &state)
{
    std::array<char, lineLimit> buffer{};
    std::string line;
    line.reserve(lineLimit);
    bool oversized = false;
    for (;;) {
        const auto count = CHILPA_READ(state->input, buffer.data(), buffer.size());
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) break;
        // I tee the original bytes before doing any log processing.
        for (std::ptrdiff_t sent = 0; sent < count;) {
            const auto written = CHILPA_WRITE(state->original, buffer.data() + sent, count - sent);
            if (written < 0 && errno == EINTR) continue;
            if (written <= 0) break;
            sent += written;
        }
        for (std::ptrdiff_t i = 0; i < count; ++i) {
            if (buffer[i] == '\n') {
                if (oversized) {
                    constexpr auto dropped = "[log entry exceeded the line limit]";
                    chilpa_developer_log_line(dropped, std::char_traits<char>::length(dropped));
                } else { chilpa_developer_log_line(line.data(), line.size()); }
                line.clear(); oversized = false;
            } else if (!oversized) {
                if (line.size() < lineLimit) line.push_back(buffer[i]);
                else { line.clear(); oversized = true; }
            }
        }
    }
    if (!line.empty() && !oversized) chilpa_developer_log_line(line.data(), line.size());
    CHILPA_CLOSE(state->original); CHILPA_CLOSE(state->input);
    { std::lock_guard<std::mutex> guard(state->mutex); state->done = true; }
    state->finished.notify_one();
}
#ifdef _WIN32
unsigned __stdcall run_reader(void *argument)
#else
void *run_reader(void *argument)
#endif
{
    std::unique_ptr<std::shared_ptr<ReaderState>> state(static_cast<std::shared_ptr<ReaderState> *>(argument));
    consume(*state);
#ifdef _WIN32
    return 0;
#else
    return nullptr;
#endif
}
}
extern "C" bool chilpa_start_log_capture()
{
    std::lock_guard<std::mutex> guard(lifecycle);
    if (installed) return false;
    auto state = std::make_shared<ReaderState>();
    int ends[2];
#ifdef _WIN32
    if (_pipe(ends, lineLimit, _O_BINARY | _O_NOINHERIT) != 0) return false;
#else
    if (pipe(ends) != 0) return false;
    fcntl(ends[0], F_SETFD, FD_CLOEXEC);
    fcntl(ends[1], F_SETFD, FD_CLOEXEC);
#endif
    state->original = CHILPA_DUP(2);
    if (state->original < 0) { CHILPA_CLOSE(ends[0]); CHILPA_CLOSE(ends[1]); return false; }
#ifndef _WIN32
    fcntl(state->original, F_SETFD, FD_CLOEXEC);
#endif
    state->input = ends[0];
    std::fflush(stderr);
    if (CHILPA_DUP2(ends[1], 2) < 0) {
        CHILPA_CLOSE(ends[0]); CHILPA_CLOSE(ends[1]); CHILPA_CLOSE(state->original);
        return false;
    }
    CHILPA_CLOSE(ends[1]);
    // I bound the reader stack to 256 KiB, plus two 4 KiB line/chunk buffers.
    auto argument = std::make_unique<std::shared_ptr<ReaderState>>(state);
#ifdef _WIN32
    reader = reinterpret_cast<HANDLE>(_beginthreadex(nullptr, 256 * 1024, run_reader, argument.get(), 0, nullptr));
    running = reader != nullptr;
#else
    pthread_attr_t attributes;
    pthread_attr_init(&attributes);
    const bool configured = pthread_attr_setstacksize(&attributes, 256 * 1024) == 0;
    running = configured && pthread_create(&reader, &attributes, run_reader, argument.get()) == 0;
    pthread_attr_destroy(&attributes);
#endif
    if (!running) {
        CHILPA_DUP2(state->original, 2); CHILPA_CLOSE(state->original); CHILPA_CLOSE(state->input);
        return false;
    }
    argument.release();
    current = state; installed = true;
    return true;
}
extern "C" void chilpa_stop_log_capture()
{
    std::lock_guard<std::mutex> guard(lifecycle);
    if (!running) return;
    std::fflush(stderr);
    while (CHILPA_DUP2(current->original, 2) < 0 && errno == EINTR) {}
    // A child may still own its inherited write end. I never wait for that child.
    std::unique_lock<std::mutex> stateGuard(current->mutex);
    const bool drained = current->finished.wait_for(stateGuard, std::chrono::milliseconds(250), [] { return current->done; });
    stateGuard.unlock();
#ifdef _WIN32
    if (drained) WaitForSingleObject(reader, INFINITE);
    CloseHandle(reader); reader = nullptr;
#else
    if (drained) pthread_join(reader, nullptr); else pthread_detach(reader);
#endif
    running = false;
    // A detached reader owns its descriptors until EOF or process exit.
    current.reset();
}
