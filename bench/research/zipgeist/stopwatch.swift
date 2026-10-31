// stopwatch <bundle-path> <arm-file> <result-file> [skip]
// Waits for <arm-file>, lets <skip> left clicks go by (the ones that open the menu
// and the dialog), then times from the next left click until every process of the
// app (and anything it starts) has gone idle. Prints and appends one JSON line.
import CoreGraphics
import Darwin
import Foundation

let args = CommandLine.arguments
let bundle = args[1], armFile = args[2], resultFile = args[3]
let skip = args.count > 4 ? Int(args[4]) ?? 0 : 0
var timebase = mach_timebase_info_data_t()
mach_timebase_info(&timebase)
func ticksToSeconds(_ t: UInt64) -> Double { Double(t) * Double(timebase.numer) / Double(timebase.denom) / 1e9 }
func now() -> Double { Date().timeIntervalSince1970 }

func path(_ pid: pid_t) -> String {
    var buf = [CChar](repeating: 0, count: 4096)
    return proc_pidpath(pid, &buf, 4096) > 0 ? String(cString: buf) : ""
}
func parent(_ pid: pid_t) -> pid_t {
    var info = proc_bsdinfo()
    return proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, Int32(MemoryLayout<proc_bsdinfo>.size)) > 0 ? pid_t(info.pbi_ppid) : 0
}
func usage(_ pid: pid_t) -> (cpu: Double, mem: Double)? {
    var ri = rusage_info_v4()
    let ok = withUnsafeMutablePointer(to: &ri) {
        $0.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { proc_pid_rusage(pid, RUSAGE_INFO_V4, $0) }
    }
    guard ok == 0 else { return nil }
    return (ticksToSeconds(ri.ri_user_time + ri.ri_system_time), Double(ri.ri_phys_footprint) / 1_048_576)
}
/// The app's processes and their descendants.
func tree() -> [pid_t] {
    let count = proc_listallpids(nil, 0)
    var pids = [pid_t](repeating: 0, count: Int(count) + 64)
    let n = proc_listallpids(&pids, Int32(pids.count * MemoryLayout<pid_t>.size))
    var parents: [pid_t: pid_t] = [:], roots = Set<pid_t>()
    for pid in pids.prefix(Int(n)) where pid > 0 {
        parents[pid] = parent(pid)
        if path(pid).hasPrefix(bundle) { roots.insert(pid) }
    }
    return parents.keys.filter { pid in
        var p = pid
        for _ in 0..<6 { if roots.contains(p) { return true }; p = parents[p] ?? 0; if p <= 1 { break } }
        return false
    }
}

// Arm, then wait for the clicks.
while !FileManager.default.fileExists(atPath: armFile) { usleep(10_000) }
try? FileManager.default.removeItem(atPath: armFile)
let armed = now()
var clicks: [Double] = []
while clicks.count <= skip {
    let since = CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: .leftMouseDown)
    let at = now() - since
    // the same click reads back a millisecond or so apart from poll to poll
    if at > armed + 0.01, clicks.last.map({ at - $0 > 0.05 }) ?? true { clicks.append(at) }
    usleep(2_000)
}
let click = clicks.last!

var last: [pid_t: Double] = [:], cpu = 0.0, peak = 0.0, lastBusy = 0.0, scanned = 0.0
var members = tree()
for pid in members { if let u = usage(pid) { last[pid] = u.cpu } }
while true {
    usleep(20_000)
    let t = now()
    if t - scanned > 0.2 { members = tree(); scanned = t }
    var delta = 0.0, mem = 0.0
    for pid in members {
        guard let u = usage(pid) else { continue }
        delta += max(0, u.cpu - (last[pid] ?? 0))
        last[pid] = u.cpu
        mem += u.mem
    }
    cpu += delta
    peak = max(peak, mem)
    if delta > 0.004 { lastBusy = t }
    if lastBusy > 0, t - lastBusy > 3 { break }
    if lastBusy == 0, t - click > 20 { break }
    if t - click > 1800 { break }
}
let wall = lastBusy > 0 ? lastBusy - click : 0
let line = String(format: "{\"wall\": %.2f, \"cpu\": %.2f, \"peak_mb\": %.0f, \"click\": %.3f, \"skipped\": %d}", wall, cpu, peak, click, clicks.count - 1)
print(line)
if let h = FileHandle(forWritingAtPath: resultFile) { h.seekToEndOfFile(); h.write((line + "\n").data(using: .utf8)!); try? h.close() }
else { try? (line + "\n").write(toFile: resultFile, atomically: true, encoding: .utf8) }
