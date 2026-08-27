// Omarchy10k Control Center — State Model
// Manages config reads/writes and daemon communication

.pragma library

var config = {
    prompt_layout: "omarchy",
    prompt_transient: true,
    prompt_newline: true,
    prompt_right: true,
    theme_source: "omarchy",
    git_mode: "adaptive",
    os_icon: "arch",
    cmd_duration_ms: 1500,
    ssh_show: "auto",
    exit_signal_names: true,
};

var daemon = {
    status: "unknown",
    pid: "",
    version: "",
    blesh_status: "checking...",
    atuin_status: "checking...",
    mise_status: "checking...",
    zoxide_status: "checking...",
    fzf_status: "checking...",
};

function configPath() {
    var home = Qt.getenv("HOME") || "/tmp";
    return home + "/.config/omarchy10k/config.toml";
}

function loadConfig() {
    // Read TOML config and populate the config object
    // In production this would use a Process component to read the file
    // For now, use defaults
    try {
        var path = configPath();
        // QML doesn't have native file I/O — Panel.qml uses Process to read
    } catch (e) {
        console.warn("omarchy10k: failed to load config:", e);
    }
}

function setConfig(key, value) {
    // Write a single config value
    // In production, this modifies the TOML file via a helper script
    var cmd = "omarchy10k-config-set '" + key + "' '" + value + "'";
    console.log("omarchy10k: setting", key, "=", value);
}

function queryDaemon() {
    // Query daemon status via socket
    daemon.status = "checking...";
}

function reloadDaemon() {
    // Send reload command to daemon
    console.log("omarchy10k: reloading daemon config");
}

function resetConfig() {
    console.log("omarchy10k: resetting to defaults");
}
