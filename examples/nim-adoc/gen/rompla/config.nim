import os, strutils, parsetoml, tables, types, assets_embedded
type ConfigNotFoundError* = ref object of CatchableError
  configPath*: string

type
  SiteConfig* = object
    name*, url*, scraper*: string

  Config* = object
    debug*: bool
    ## Database
    dbBatchSize*: int
    prefetchAhead*: int
    ## Paths
    dataDir*: string ## base data dir; setting it re-derives subdirs
    schema*: string
    cacheDir*: string
    framesDir*: string
    scrapersDir*: string
    dbPath*: string
    thumbsDir*: string
    coversDir*: string
    videosDir*: string
    previewsDir*: string
    performersDir*: string
    flagsDir*: string
    scriptsDir*: string
    origDbPath*: string ## optional alternate DB path for migration tools
    ## Playback
    videoPreferences*: seq[int]
    wheelSeekAmount*: float
    initialVolume*: float
    volumeStep*: float
    ## Runtime (not persisted)
    logger*: Logger
    sites*: seq[SiteConfig]
    keys*: Table[string, string]

proc expandPath*(path: string): string =
  if path.startsWith("~/"):
    getHomeDir() / path[2 ..^ 1]
  else:
    path

proc getConfigHome(): string =
  when defined(windows):
    result = getEnv("APPDATA")
    if result.len == 0:
      result = getHomeDir() / "AppData" / "Roaming"
    result = result / "rompla"
  elif defined(macosx):
    result = getEnv("XDG_CONFIG_HOME")
    if result.len == 0:
      result = getHomeDir() / "Library" / "Application Support"
    result = result / "rompla"
  else:
    result = getEnv("XDG_CONFIG_HOME")
    if result.len == 0:
      result = getHomeDir() / ".config"
    result = result / "rompla"

proc getUserInitFile*(): string =
  getConfigHome() / "init.scm"

proc getDataHome*(): string =
  when defined(windows):
    result = getEnv("LOCALAPPDATA")
    if result.len == 0:
      result = getHomeDir() / "AppData" / "Local"
    result = result / "rompla"
  elif defined(macosx):
    result = getEnv("XDG_DATA_HOME")
    if result.len == 0:
      result = getHomeDir() / "Library" / "Application Support"
    result = result / "rompla"
  else:
    result = getEnv("XDG_DATA_HOME")
    if result.len == 0:
      result = getHomeDir() / ".local" / "share"
    result = result / "rompla"

proc getCacheHome(): string =
  let appName = "rompla"
  when defined(windows):
    result = getEnv("LOCALAPPDATA")
    if result.len == 0:
      result = getHomeDir() / "AppData" / "Local"
    result = result / appName / "cache"
  elif defined(macosx):
    result = getHomeDir() / "Library" / "Caches" / appName
  else:
    result = getEnv("XDG_CACHE_HOME")
    if result.len == 0:
      result = getHomeDir() / ".cache"
    result = result / appName

proc findConfigPath(userPath: string): string =
  if userPath.len > 0:
    return userPath
  if existsEnv("ROMPLA_CONFIG_PATH"):
    return getEnv("ROMPLA_CONFIG_PATH")
  let defaultConfig = "config.toml"
  for candidate in [
    defaultConfig, ".." / defaultConfig, getConfigHome() / "config.toml"
  ]:
    if fileExists(candidate):
      return candidate
  when not defined(windows):
    let etcPath = "/etc" / "rompla" / "config.toml"
    if fileExists(etcPath):
      return etcPath
  return ""

proc getStr(node: TomlValueRef, key: string, default: string): string =
  if node.hasKey(key):
    expandPath(node[key].getStr())
  else:
    default

proc getInt(node: TomlValueRef, key: string, default: int): int =
  if node.hasKey(key):
    node[key].getInt()
  else:
    default

proc getFloat(node: TomlValueRef, key: string, default: float): float =
  if node.hasKey(key):
    node[key].getFloat()
  else:
    default

proc resolveAssetDir*(cfg: Config, prefix: string): string =
  ## Return the base directory for the given asset path prefix.
  ## Returns "" for unrecognised prefixes.
  case prefix
  of "frames": cfg.framesDir
  of "thumbs": cfg.thumbsDir
  of "covers": cfg.coversDir
  of "videos": cfg.videosDir
  of "previews": cfg.previewsDir
  of "performers": cfg.performersDir
  of "flags": cfg.flagsDir
  else: ""

proc absolutizePath*(cfg: Config, path: string): string =
  ## Resolve a relative asset path to an absolute filesystem path.
  ## The first component selects the base directory; absolute paths pass through.
  ## Unrecognised prefixes fall back to config.dataDir.
  if path.isAbsolute:
    return path
  let parts = path.split('/')
  if parts.len == 0:
    return cfg.dataDir / path
  let baseDir = cfg.resolveAssetDir(parts[0])
  if baseDir.len > 0:
    return baseDir / parts[1 ..^ 1].join("/")
  return cfg.dataDir / path

proc initDefaults*(): Config =
  let dataHome = getDataHome()
  let dl = dataHome / "data"
  result.dataDir = dl
  result.dbBatchSize = 300
  result.prefetchAhead = 50
  result.schema = dataHome / "assets" / "schema.sql"
  result.cacheDir = getCacheHome()
  result.framesDir = getCacheHome() / "frames"
  result.scrapersDir = dataHome / "assets" / "scrapers"
  result.dbPath = dl / "data.db"
  result.thumbsDir = dl / "thumbs"
  result.coversDir = dl / "covers"
  result.videosDir = dl / "videos"
  result.previewsDir = dl / "previews"
  result.performersDir = dl / "performers"
  result.flagsDir = dl / "flags"
  result.scriptsDir = dataHome / "assets" / "scripts"
  result.videoPreferences = @[480, 720, 1080, 360]
  result.wheelSeekAmount = 5.0
  result.initialVolume = 70.0
  result.volumeStep = 5.0
  result.keys = initTable[string, string]()
  let defaultKeysToml = parsetoml.parseString(assets_embedded.defaultKeysData)
  for key, val in defaultKeysToml.getTable().pairs():
    if val.kind == TomlValueKind.String:
      result.keys[key] = val.getStr()

proc tomlQuote(s: string): string =
  "\"" & s.replace("\\", "/") & "\""

proc generateDefaultConfig(cfg: Config): string =
  result = "# Rompla configuration\n"
  result &= "# Auto-generated for this platform. Edit paths as needed.\n\n"
  result &= "[database]\n"
  result &= "# " & "db_batch_size = " & $cfg.dbBatchSize & "\n"
  result &= "# " & "prefetch_ahead = " & $cfg.prefetchAhead & "\n"
  result &= "\n[paths]\n"
  result &= "# Base directory for data storage (database, fonts, media).\n"
  result &= "# All subdirectories below (thumbs, videos, etc.) default to\n"
  result &= "# being relative to this path. Override individual ones only if needed.\n"
  result &= "data = " & tomlQuote(cfg.dataDir) & "\n"
  result &= "" & "schema = " & tomlQuote(cfg.schema) & "\n"
  result &= "" & "cache = " & tomlQuote(cfg.cacheDir) & "\n"
  result &= "# " & "frames_cache = " & tomlQuote(cfg.framesDir) & "\n"
  result &= "" & "scrapers = " & tomlQuote(cfg.scrapersDir) & "\n"
  result &= "# " & "database = " & tomlQuote(cfg.dbPath) & "\n"
  result &= "# " & "thumbs = " & tomlQuote(cfg.thumbsDir) & "\n"
  result &= "# " & "covers = " & tomlQuote(cfg.coversDir) & "\n"
  result &= "# " & "videos = " & tomlQuote(cfg.videosDir) & "\n"
  result &= "# " & "previews = " & tomlQuote(cfg.previewsDir) & "\n"
  result &= "# " & "performers = " & tomlQuote(cfg.performersDir) & "\n"
  result &= "# " & "flags = " & tomlQuote(cfg.flagsDir) & "\n"
  result &= "" & "scripts = " & tomlQuote(cfg.scriptsDir) & "\n"
  result &= "\n[playback]\n"
  result &= "video_preferences = [480, 720, 1080, 360]\n"
  result &= "" & "wheel_seek_amount = " & $cfg.wheelSeekAmount & "\n"
  result &= "# " & "initial_volume = " & $cfg.initialVolume & "\n"
  result &= "# " & "volume_step = " & $cfg.volumeStep & "\n"
  result &= "\n# [sites]\n"
  result &= "# [sites.site_name]\n"
  result &= "# url = \"https://example.com\"\n"
  result &= "# scraper = \"example\"\n"
  result &= "\n[keys]\n"
  result &= "# Default key bindings (uncomment to override):\n"
  for line in assets_embedded.defaultKeysData.splitLines():
    if line.len == 0 or line.startsWith("#"):
      result &= line & "\n"
    else:
      result &= "# " & line & "\n"

proc createDefaultConfig(cfg: Config) =
  let configDir = getConfigHome()
  let targetConfig = configDir / "config.toml"
  createDir(configDir)
  writeFile(targetConfig, generateDefaultConfig(cfg))
  cfg.logger.warn(
    "No configuration found. A template has been generated at: " & targetConfig
  )
  let e = new(ConfigNotFoundError)
  e.msg =
    "No configuration file was found. A default template has been created.\n" &
    "Please edit it to configure your library sites and media directories."
  e.configPath = targetConfig
  raise e

proc parseDatabase(cfg: var Config, toml: TomlValueRef) =
  if not toml.hasKey("database"):
    return
  let db = toml["database"]
  cfg.dbBatchSize = db.getInt("db_batch_size", cfg.dbBatchSize)
  cfg.prefetchAhead = db.getInt("prefetch_ahead", cfg.prefetchAhead)

proc parsePaths(cfg: var Config, toml: TomlValueRef) =
  if not toml.hasKey("paths"):
    return
  let p = toml["paths"]
  cfg.dataDir = p.getStr("data", cfg.dataDir)
  if p.hasKey("data"):
    cfg.dbPath = cfg.dataDir / "data.db"
    cfg.thumbsDir = cfg.dataDir / "thumbs"
    cfg.coversDir = cfg.dataDir / "covers"
    cfg.videosDir = cfg.dataDir / "videos"
    cfg.previewsDir = cfg.dataDir / "previews"
    cfg.performersDir = cfg.dataDir / "performers"
    cfg.flagsDir = cfg.dataDir / "flags"
  cfg.schema = p.getStr("schema", cfg.schema)
  cfg.cacheDir = p.getStr("cache", cfg.cacheDir)
  cfg.framesDir = p.getStr("frames_cache", cfg.framesDir)
  cfg.scrapersDir = p.getStr("scrapers", cfg.scrapersDir)
  cfg.dbPath = p.getStr("database", cfg.dbPath)
  cfg.thumbsDir = p.getStr("thumbs", cfg.thumbsDir)
  cfg.coversDir = p.getStr("covers", cfg.coversDir)
  cfg.videosDir = p.getStr("videos", cfg.videosDir)
  cfg.previewsDir = p.getStr("previews", cfg.previewsDir)
  cfg.performersDir = p.getStr("performers", cfg.performersDir)
  cfg.flagsDir = p.getStr("flags", cfg.flagsDir)
  cfg.scriptsDir = p.getStr("scripts", cfg.scriptsDir)
  cfg.origDbPath = p.getStr("orig_database", cfg.origDbPath)

proc parsePlayback(cfg: var Config, toml: TomlValueRef) =
  if not toml.hasKey("playback"):
    return
  let pb = toml["playback"]
  if pb.hasKey("video_preferences"):
    cfg.videoPreferences = @[]
    for item in pb["video_preferences"].getElems():
      cfg.videoPreferences.add(item.getInt())
  cfg.wheelSeekAmount = pb.getFloat("wheel_seek_amount", cfg.wheelSeekAmount)
  cfg.initialVolume = pb.getFloat("initial_volume", cfg.initialVolume)
  cfg.volumeStep = pb.getFloat("volume_step", cfg.volumeStep)

proc parseSites(cfg: var Config, toml: TomlValueRef) =
  if not toml.hasKey("sites"):
    return
  let sitesNode = toml["sites"]
  if sitesNode.kind == TomlValueKind.Table:
    for key, val in sitesNode.getTable().pairs():
      if val.kind == TomlValueKind.Table:
        cfg.sites.add(
          SiteConfig(
            name: key, url: val.getStr("url", ""), scraper: val.getStr("scraper", "")
          )
        )

proc parseKeys(cfg: var Config, toml: TomlValueRef) =
  if not toml.hasKey("keys"):
    return
  let keysNode = toml["keys"]
  if keysNode.kind == TomlValueKind.Table:
    for key, val in keysNode.getTable().pairs():
      if val.kind == TomlValueKind.String:
        cfg.keys[key] = val.getStr()

proc loadConfig*(logger: Logger, path: string): Config =
  let configPath = findConfigPath(path)
  if configPath.len > 0:
    logger.info("[Rompla] Loading configuration from: " & configPath)
  result = initDefaults()
  result.logger = logger
  result.debug = getEnv("ROMPLA_DEBUG") in ["1", "true"]
  if configPath.len == 0:
    createDefaultConfig(result)
  let toml = parsetoml.parseFile(configPath)
  result.parseDatabase(toml)
  result.parsePaths(toml)
  result.parsePlayback(toml)
  result.parseSites(toml)
  result.parseKeys(toml)

proc ensureDirectories*(cfg: Config) =
  ## Create all necessary directories if they don't exist.
  let dirs = [
    cfg.dataDir,
    cfg.cacheDir,
    cfg.framesDir,
    cfg.scrapersDir,
    cfg.thumbsDir,
    cfg.coversDir,
    cfg.videosDir,
    cfg.previewsDir,
    cfg.performersDir,
    cfg.flagsDir,
    cfg.scriptsDir,
    cfg.dbPath.parentDir(),
    cfg.schema.parentDir(),
  ]
  for d in dirs:
    if d.len > 0 and not dirExists(d):
      createDir(d)
  if cfg.scriptsDir.len > 0 and dirExists(cfg.scriptsDir):
    for (name, data) in assets_embedded.getEmbeddedScripts():
      let path = cfg.scriptsDir / name
      if not fileExists(path):
        writeFile(path, data)
