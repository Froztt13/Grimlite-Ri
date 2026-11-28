package grimlite.host {
import flash.display.DisplayObject;
import flash.display.DisplayObjectContainer;
import flash.display.Loader;
import flash.display.MovieClip;
import flash.display.Stage;
import flash.display.StageScaleMode;
import flash.events.ErrorEvent;
import flash.events.Event;
import flash.events.IOErrorEvent;
import flash.events.MouseEvent;
import flash.events.SecurityErrorEvent;
import flash.events.UncaughtErrorEvent;
import flash.external.ExternalInterface;
import flash.net.URLLoader;
import flash.net.URLRequest;
import flash.system.Security;
import flash.utils.setTimeout;
import grimoire.Externalizer;
import grimoire.Root;
import grimoire.game.Bank;
import grimoire.game.Inventory;
import grimoire.game.Player;
import grimoire.game.Quests;
import grimoire.game.Settings;
import grimoire.game.Shops;
import grimoire.game.TempInventory;
import grimoire.game.World;

/**
 * Grimlite Ri host SWF entry point for Ruffle.
 * Loads AQW's `Game.swf`, populates `grimoire.Root`, registers all Grimlite-Li callbacks
 * via `grimoire.Externalizer`, and exposes Grimlite Ri's typed lowercase helpers.
 */
public class Main extends MovieClip {
    private static const VERSION:int = 4;
    private static const GAME_URL:String = "https://game.aq.com/game/";
    private static const TITLE:String = '<font color="#E63946">Grimlite Ri</font>';
    private static const VERSION_TRIES:int = 3;

    private var stageRef:Stage;
    private var loading:Boolean = false;
    private var versionTries:int = 0;
    private var vars:Object;
    private var loader:Loader;
    private var game:*;
    private var paramsSet:Boolean = false;
    private var callbacks:Array = [];

    private var optDisableAnimations:Boolean = false;
    private var optHidePlayers:Boolean = false;
    private var optSkipCutscenes:Boolean = false;
    private var optEnemyMagnet:Boolean = false;
    private var optWalkSpeedEnabled:Boolean = false;
    private var optWalkSpeed:int = 8;
    private var stateFrameCounter:int = 0;
    private var optFrameCounter:int = 0;
    private var hasPushedState:Boolean = false;
    private var lastLoggedIn:Boolean = false;
    private var lastUsername:String = "";
    private var lastHp:int = 0;
    private var lastMaxHp:int = 0;
    private var lastMp:int = 0;
    private var lastMaxMp:int = 0;
    private var lastLevel:int = 0;
    private var lastGold:int = 0;
    private var lastMap:String = "";
    private var lastCell:String = "";
    private var lastPad:String = "";
    private var lastState:int = 0;
    private var lastAlive:Boolean = false;
    private var lastTargetName:String = "";
    private var lastTargetHp:int = 0;
    private var lastTargetMaxHp:int = 0;

    public function Main() {
        super();
        if (stage != null) {
            start();
        } else {
            addEventListener(Event.ADDED_TO_STAGE, onAddedToStage);
        }
    }

    private function onAddedToStage(event:Event):void {
        removeEventListener(Event.ADDED_TO_STAGE, onAddedToStage);
        start();
    }

    private function start():void {
        stageRef = stage;
        Root.StageRef = stage;
        try {
            Security.allowDomain("*");
        } catch (e:Error) {}

        patchTrim();
        loaderInfo.uncaughtErrorEvents.addEventListener(UncaughtErrorEvent.UNCAUGHT_ERROR, onUncaughtError);

        Externalizer.registerAll();
        registerHostCallbacks();

        callHost("requestLoadGame", []);
        setTimeout(function():void {
            if (!loading && game == null) {
                loadClient();
            }
        }, 150);
    }

    private function registerHostCallbacks():void {
        // Lifecycle & State
        expose("hostInfo", hostInfo);
        expose("loadClient", loadClient);
        expose("isLoggedIn", isLoggedIn);
        expose("username", username);
        expose("getPlayerState", getPlayerState);
        expose("login", login);
        expose("clickServer", clickServer);

        // Options
        expose("setInfiniteRange", setInfiniteRange);
        expose("aggroMonsters", aggroMonsters);
        expose("setEnemyMagnet", setEnemyMagnet);
        expose("setLagKiller", setLagKiller);
        expose("setLagKillerImage", setLagKillerImage);
        expose("setHidePlayers", setHidePlayers);
        expose("setDisableAnimations", setDisableAnimations);
        expose("setSkipCutscenes", setSkipCutscenes);
        expose("setFPS", setFPS);
        expose("setWalkSpeed", setWalkSpeed);
        expose("setWalkSpeedOpt", setWalkSpeedOpt);

        // Map
        expose("joinMap", joinMap);
        expose("moveToCell", moveToCell);
        expose("walkTo", walkTo);
        expose("getPosition", getPosition);
        expose("setSpawnPoint", setSpawnPoint);
        expose("isMapLoading", isMapLoading);
        expose("getCells", getCells);

        // Combat
        expose("getCombatState", getCombatState);
        expose("attackMonster", attackMonster);
        expose("useSkill", useSkill);
        expose("forceUseSkill", forceUseSkill);
        expose("skillAvailable", skillAvailable);
        expose("cancelTarget", cancelTarget);
        expose("rest", rest);
        expose("isMonsterAvailable", isMonsterAvailable);
        expose("hasTarget", hasTarget);

        // Inventory / Bank / Shop
        expose("hasItem", hasItem);
        expose("hasTempItem", hasTempItem);
        expose("hasBankItem", hasBankItem);
        expose("equipItem", equipItem);
        expose("isEquipped", isEquipped);
        expose("loadShop", loadShop);
        expose("isShopLoaded", isShopLoaded);
        expose("buyItem", buyItem);
        expose("sellItem", sellItem);
        expose("transferToBank", transferToBank);
        expose("transferToInv", transferToInv);
        expose("openBank", openBank);
        expose("getMapItem", getMapItem);

        // Quests
        expose("loadQuest", loadQuest);
        expose("isQuestLoaded", isQuestLoaded);
        expose("isQuestCompletedOnce", isQuestCompletedOnce);
        expose("acceptQuest", acceptQuest);
        expose("completeQuest", completeQuest);
        expose("isQuestInProgress", isQuestInProgress);
        expose("canCompleteQuest", canCompleteQuest);
    }

    private static function patchTrim():void {
        var proto:Object = String.prototype;
        proto.trim = function():String {
            return String(this).replace(/^\s+|\s+$/g, "");
        };
        proto.setPropertyIsEnumerable("trim", false);
    }

    private function expose(name:String, fn:Function):void {
        callbacks.push(name);
        if (ExternalInterface.available) {
            ExternalInterface.addCallback(name, fn);
        }
    }

    private function callHost(name:String, args:Array):void {
        if (ExternalInterface.available) {
            ExternalInterface.call.apply(ExternalInterface, [name].concat(args));
        }
    }

    private function onUncaughtError(event:UncaughtErrorEvent):void {
        event.preventDefault();
        var msg:String = (event.error is Error) ? Error(event.error).message : String(event.error);
        callHost("debug", ["[AS3 Error] " + msg]);
    }

    // ---- Host Info & Client Loading ----

    private function hostInfo():String {
        return JSON.stringify({
            name: "grimlite-ri",
            version: VERSION,
            base: "grimlite-li-swf",
            callbacks: callbacks
        });
    }

    private function loadClient(frameRate:* = undefined):void {
        var fps:Number = Number(frameRate);
        if (fps > 0 && stageRef != null) {
            stageRef.frameRate = fps;
        }
        if (loading) {
            return;
        }
        loading = true;
        fetchVersion();
        callHost("pre-load", []);
    }

    private function fetchVersion():void {
        versionTries++;
        var fetch:URLLoader = new URLLoader();
        fetch.addEventListener(Event.COMPLETE, onVersion);
        fetch.addEventListener(IOErrorEvent.IO_ERROR, onVersionFailed);
        fetch.addEventListener(SecurityErrorEvent.SECURITY_ERROR, onVersionFailed);
        fetch.load(new URLRequest(GAME_URL + "api/data/gameversion"));
    }

    private function onVersionFailed(event:ErrorEvent):void {
        if (versionTries < VERSION_TRIES) {
            callHost("debug", ["Retrying gameversion: " + event.text]);
            setTimeout(fetchVersion, 2000);
        } else {
            callHost("debug", ["Failed to fetch gameversion: " + event.text]);
            loading = false;
        }
    }

    private function onVersion(event:Event):void {
        try {
            vars = JSON.parse(String(URLLoader(event.target).data));
        } catch (e:Error) {
            callHost("debug", ["Invalid gameversion JSON: " + e.message]);
            loading = false;
            return;
        }
        loader = new Loader();
        loader.contentLoaderInfo.addEventListener(Event.INIT, onClientInit);
        loader.contentLoaderInfo.addEventListener(Event.COMPLETE, onClientLoaded);
        loader.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, onClientFailed);
        loader.load(new URLRequest(GAME_URL + "gamefiles/" + vars.sFile + "?ver=" + Math.random()));
    }

    private function onClientFailed(event:IOErrorEvent):void {
        callHost("debug", ["Failed to load Game.swf: " + event.text]);
        loading = false;
    }

    private function onClientInit(event:Event):void {
        game = loader.content;
        Root.Game = game;
        Root.GameDomain = loader.contentLoaderInfo.applicationDomain;
        setParams();
    }

    private function setParams():void {
        if (game == null || game.params == null) {
            return;
        }
        var params:Object = game.params;
        params.vars = vars;
        params.sURL = GAME_URL;
        params.sBG = "Generic2.swf";
        params.sTitle = TITLE;
        params.isEU = false;
        params.loginURL = GAME_URL + "api/login/now";
        paramsSet = true;
    }

    private function onClientLoaded(event:Event):void {
        game = loader.content;
        Root.Game = game;
        Root.GameDomain = loader.contentLoaderInfo.applicationDomain;
        if (!paramsSet) {
            setParams();
        }
        if (this.parent == stageRef) {
            stageRef.removeChild(this);
        }
        stageRef.addChild(DisplayObject(game));
        stageRef.scaleMode = StageScaleMode.SHOW_ALL;
        stageRef.align = "";
        game.addEventListener(Event.ENTER_FRAME, onFrame);
        callHost("loaded", []);
    }

    private function onFrame(event:Event):void {
        stateFrameCounter++;
        if (stateFrameCounter >= 12) {
            stateFrameCounter = 0;
            pushPlayerStateIfChanged();
        }
        if (game == null || game.world == null) {
            return;
        }
        if (optWalkSpeedEnabled && int(game.world.WALKSPEED) != optWalkSpeed) {
            game.world.WALKSPEED = optWalkSpeed;
        }
        optFrameCounter++;
        if (optFrameCounter >= 3) {
            optFrameCounter = 0;
            if (optSkipCutscenes) {
                Settings.SetSkipCutscenes();
            }
            if (optEnemyMagnet) {
                Settings.SetEnemyMagnet();
            }
            if (optHidePlayers) {
                Settings.SetHidePlayers(true);
            }
            if (optDisableAnimations) {
                Settings.DisableAnimationsFrame();
            }
        }
    }

    private function pushPlayerStateIfChanged():void {
        var loggedIn:Boolean = false;
        var uname:String = "";
        var hp:int = 0;
        var maxHp:int = 0;
        var mp:int = 0;
        var maxMp:int = 0;
        var level:int = 0;
        var gold:int = 0;
        var mapName:String = "";
        var cellName:String = "";
        var padName:String = "";
        var pState:int = 0;
        var alive:Boolean = false;
        var targetName:String = "";
        var targetHp:int = 0;
        var targetMaxHp:int = 0;
        try {
            if (game != null && game.sfc != null && game.sfc.isConnected && game.world != null && game.world.myAvatar != null) {
                var av:* = game.world.myAvatar;
                var data:* = av.objData;
                var leaf:* = av.dataLeaf;
                loggedIn = true;
                if (data != null) {
                    uname = String(data.strUsername || "");
                    level = int(data.intLevel);
                    gold = int(data.intGold);
                }
                if (leaf != null) {
                    hp = int(leaf.intHP);
                    maxHp = int(leaf.intHPMax);
                    mp = int(leaf.intMP);
                    maxMp = int(leaf.intMPMax);
                    pState = int(leaf.intState);
                    alive = hp > 0 && pState > 0;
                } else if (data != null) {
                    hp = int(data.intHP);
                    maxHp = int(data.intHPMax);
                    mp = int(data.intMP);
                    maxMp = int(data.intMPMax);
                    pState = int(data.intState);
                    alive = maxHp == 0 || (hp > 0 && pState > 0);
                }
                mapName = String(game.world.strMapName || "");
                cellName = String(game.world.strFrame || "");
                padName = String(game.world.strPad || "");
                if (av.target != null) {
                    if (av.target.objData != null) {
                        targetName = String(av.target.objData.strMonName || av.target.objData.strUsername || "");
                    }
                    var tLeaf:* = av.target.dataLeaf != null ? av.target.dataLeaf : av.target.objData;
                    if (tLeaf != null) {
                        targetHp = int(tLeaf.intHP);
                        targetMaxHp = int(tLeaf.intHPMax);
                    }
                }
            }
        } catch (e:Error) {}

        if (hasPushedState &&
            loggedIn == lastLoggedIn &&
            hp == lastHp &&
            maxHp == lastMaxHp &&
            mp == lastMp &&
            maxMp == lastMaxMp &&
            level == lastLevel &&
            gold == lastGold &&
            pState == lastState &&
            alive == lastAlive &&
            targetHp == lastTargetHp &&
            targetMaxHp == lastTargetMaxHp &&
            uname == lastUsername &&
            mapName == lastMap &&
            cellName == lastCell &&
            padName == lastPad &&
            targetName == lastTargetName) {
            return;
        }

        hasPushedState = true;
        lastLoggedIn = loggedIn;
        lastUsername = uname;
        lastHp = hp;
        lastMaxHp = maxHp;
        lastMp = mp;
        lastMaxMp = maxMp;
        lastLevel = level;
        lastGold = gold;
        lastMap = mapName;
        lastCell = cellName;
        lastPad = padName;
        lastState = pState;
        lastAlive = alive;
        lastTargetName = targetName;
        lastTargetHp = targetHp;
        lastTargetMaxHp = targetMaxHp;

        var json:String = JSON.stringify({
            loggedIn: loggedIn,
            username: uname,
            hp: hp,
            maxHp: maxHp,
            mp: mp,
            maxMp: maxMp,
            level: level,
            gold: gold,
            map: mapName,
            cell: cellName,
            pad: padName,
            state: pState,
            alive: alive,
            targetName: targetName,
            targetHp: targetHp,
            targetMaxHp: targetMaxHp
        });
        callHost("playerState", [json]);
    }

    // ---- Auth & Player State ----

    private function login(user:String, pass:String):String {
        try {
            Root.Username = user;
            Root.Password = pass;
            if (game != null && game.login is Function) {
                game.login(user, pass);
                return "true";
            }
        } catch (e:Error) {}
        return "false";
    }

    private function clickServer(name:String):String {
        try {
            if (game != null && game.mcLogin != null && game.mcLogin.sl != null && game.mcLogin.sl.iList != null) {
                var list:DisplayObjectContainer = game.mcLogin.sl.iList;
                var wanted:String = String(name).toLowerCase();
                for (var i:int = 0; i < list.numChildren; i++) {
                    var entry:* = list.getChildAt(i);
                    try {
                        if (String(entry.tName.ti.text).toLowerCase().indexOf(wanted) >= 0) {
                            DisplayObject(entry).dispatchEvent(new MouseEvent(MouseEvent.CLICK));
                            return "true";
                        }
                    } catch (err:Error) {}
                }
            }
        } catch (e:Error) {}
        return "false";
    }

    private function isLoggedIn():String {
        return Player.IsLoggedIn() == Root.TrueString ? "true" : "false";
    }

    private function username():String {
        try {
            return String(game.world.myAvatar.objData.strUsername || "");
        } catch (e:Error) {}
        return "";
    }

    private function getPlayerState():String {
        var out:Object = {
            loggedIn: false, username: "",
            hp: 0, maxHp: 0, mp: 0, maxMp: 0,
            level: 0, gold: 0,
            map: "", cell: "", pad: "",
            state: 0, alive: false,
            targetName: "", targetHp: 0, targetMaxHp: 0
        };
        try {
            if (game != null && game.sfc != null && game.sfc.isConnected && game.world != null && game.world.myAvatar != null) {
                var av:* = game.world.myAvatar;
                var data:* = av.objData;
                var leaf:* = av.dataLeaf;
                out.loggedIn = true;
                if (data != null) {
                    out.username = String(data.strUsername || "");
                    out.level = int(data.intLevel);
                    out.gold = int(data.intGold);
                }
                if (leaf != null) {
                    out.hp = int(leaf.intHP);
                    out.maxHp = int(leaf.intHPMax);
                    out.mp = int(leaf.intMP);
                    out.maxMp = int(leaf.intMPMax);
                    out.state = int(leaf.intState);
                    out.alive = int(leaf.intHP) > 0 && int(leaf.intState) > 0;
                } else if (data != null) {
                    out.hp = int(data.intHP);
                    out.maxHp = int(data.intHPMax);
                    out.mp = int(data.intMP);
                    out.maxMp = int(data.intMPMax);
                    out.state = int(data.intState);
                    out.alive = out.maxHp == 0 || (out.hp > 0 && out.state > 0);
                }
                out.map = String(game.world.strMapName || "");
                out.cell = String(game.world.strFrame || "");
                out.pad = String(game.world.strPad || "");
                if (av.target != null) {
                    if (av.target.objData != null) {
                        out.targetName = String(av.target.objData.strMonName || av.target.objData.strUsername || "");
                    }
                    var tLeaf:* = av.target.dataLeaf != null ? av.target.dataLeaf : av.target.objData;
                    if (tLeaf != null) {
                        out.targetHp = int(tLeaf.intHP);
                        out.targetMaxHp = int(tLeaf.intHPMax);
                    }
                }
            }
        } catch (e:Error) {}
        return JSON.stringify(out);
    }

    // ---- Options ----

    private function setInfiniteRange(on:* = true):String {
        Settings.SetInfiniteRange(on == true || on == "true");
        return "true";
    }

    private function aggroMonsters():String {
        Settings.SetProvokeMonsters();
        return "true";
    }

    private function setEnemyMagnet(on:* = true):String {
        optEnemyMagnet = (on == true || on == "true");
        return "true";
    }

    private function setLagKiller(on:* = true):String {
        Settings.SetLagKiller((on == true || on == "true") ? "True" : "False");
        return "true";
    }

    private function setLagKillerImage(base64Data:String = ""):String {
        Settings.SetLagKillerImage(base64Data);
        return "true";
    }

    private function setHidePlayers(on:* = true):String {
        optHidePlayers = (on == true || on == "true");
        Settings.SetHidePlayers(optHidePlayers);
        return "true";
    }

    private function setDisableAnimations(on:* = true):String {
        optDisableAnimations = (on == true || on == "true");
        Settings.SetDisableAnimations(optDisableAnimations);
        return "true";
    }

    private function setSkipCutscenes(on:* = true):String {
        optSkipCutscenes = (on == true || on == "true");
        return "true";
    }

    private function setFPS(fps:* = 24):String {
        Root.setFPS(String(fps));
        return "true";
    }

    private function setWalkSpeed(speed:* = 8):String {
        var parsed:int = int(speed);
        if (parsed > 0) {
            optWalkSpeed = parsed;
        }
        Player.SetWalkSpeed(String(speed));
        return "true";
    }

    private function setWalkSpeedOpt(enabled:* = false, speed:* = 8):String {
        optWalkSpeedEnabled = (enabled == true || enabled == "true");
        var parsed:int = int(speed);
        optWalkSpeed = parsed > 0 ? parsed : 8;
        Player.SetWalkSpeed(String(optWalkSpeedEnabled ? optWalkSpeed : 8));
        return "true";
    }

    // ---- Map ----

    private function joinMap(map:String, cell:String = "Enter", pad:String = "Spawn"):String {
        Player.Join(map, cell, pad);
        return "true";
    }

    private function moveToCell(cell:String, pad:String = "Spawn"):String {
        Player.Jump(cell, pad);
        return "true";
    }

    private function walkTo(x:int, y:int):String {
        Player.WalkToPoint(String(x), String(y));
        return "true";
    }

    private function getPosition():String { return Player.Position(); }
    private function setSpawnPoint():String { World.SetSpawnPoint(); return "true"; }
    private function isMapLoading():String { return World.MapLoadComplete() == Root.TrueString ? "false" : "true"; }
    private function getCells():String { return World.GetCells(); }

    // ---- Combat ----

    private static function collectAuras(raw:*):Object {
        var map:Object = {};
        if (raw == null) {
            return map;
        }
        for each (var aura:Object in raw) {
            if (aura != null && aura.nam != null) {
                var key:String = String(aura.nam).toLowerCase().replace(/[\s\-_]/g, "");
                if (key.length > 0) {
                    var n:Number = parseFloat(aura.val);
                    var v:int = (!isNaN(n) && n > 0) ? int(Math.ceil(n)) : 1;
                    if (map[key] == null || v > int(map[key])) {
                        map[key] = v;
                    }
                }
            }
        }
        return map;
    }

    private function getCombatState():String {
        var out:Object = {
            className: "",
            hp: 0,
            maxHp: 1,
            mp: 0,
            hasTarget: false,
            counterAttack: false,
            ready: [false, false, false, false, false],
            selfAuras: {},
            targetAuras: {}
        };
        try {
            if (game != null && game.world != null && game.world.myAvatar != null) {
                var av:* = game.world.myAvatar;
                var data:* = av.objData;
                var leaf:* = av.dataLeaf;
                if (data != null && data.strClassName != null) {
                    out.className = String(data.strClassName);
                }
                if (leaf != null) {
                    out.hp = int(leaf.intHP);
                    out.maxHp = int(leaf.intHPMax) > 0 ? int(leaf.intHPMax) : 1;
                    out.mp = int(leaf.intMP);
                    out.selfAuras = collectAuras(leaf.auras);
                } else if (data != null) {
                    out.hp = int(data.intHP);
                    out.maxHp = int(data.intHPMax) > 0 ? int(data.intHPMax) : 1;
                    out.mp = int(data.intMP);
                }
                var hasTgt:Boolean = av.target != null && av.target.dataLeaf != null && int(av.target.dataLeaf.intHP) > 0;
                out.hasTarget = hasTgt;
                if (hasTgt) {
                    var tAuras:* = av.target.dataLeaf.auras;
                    out.targetAuras = collectAuras(tAuras);
                    if (tAuras != null) {
                        for each (var ta:Object in tAuras) {
                            if (ta != null && ta.nam != null && String(ta.nam).toLowerCase().indexOf("counter attack") > -1) {
                                out.counterAttack = true;
                                break;
                            }
                        }
                    }
                }
                var cmc:Number = 1.0;
                if (leaf != null && leaf.sta != null && leaf.sta.$cmc != null) {
                    var parsedCmc:Number = Number(leaf.sta.$cmc);
                    if (!isNaN(parsedCmc)) {
                        cmc = parsedCmc < 0 ? 0 : parsedCmc;
                    }
                }
                var active:Array = (game.world.actions != null) ? (game.world.actions.active as Array) : null;
                if (active != null) {
                    for (var i:int = 0; i < 5; i++) {
                        var sk:Object = active[i];
                        if (sk == null) {
                            continue;
                        }
                        var baseMp:Number = Number(sk.mp || 0);
                        var reqMp:int = baseMp > 0 ? int(Math.round(baseMp * cmc)) : 0;
                        var enoughMp:Boolean = reqMp <= 0 || out.mp >= reqMp;
                        if (sk.isOK && !sk.skillLock && enoughMp && Player.SkillAvailable(String(i)) == 0) {
                            out.ready[i] = true;
                        }
                    }
                }
            }
        } catch (e:Error) {}
        return JSON.stringify(out);
    }

    private function isMonsterAvailable(name:String = "*"):String {
        return World.IsMonsterAvailable(name) == Root.TrueString ? "true" : "false";
    }

    private function attackMonster(name:String = "*"):String {
        if (World.GetMonsterByName(name) == null) return "false";
        Player.AttackMonster(name);
        return "true";
    }

    private function useSkill(index:String):String { Player.UseSkill(index); return "true"; }
    private function forceUseSkill(index:String):String { Player.ForceUseSkill(index); return "true"; }
    private function skillAvailable(index:String):String { return String(Player.SkillAvailable(index)); }
    private function rest(full:* = false):String { Player.Rest(); return "true"; }
    private function hasTarget():String { return Player.HasTarget() == Root.TrueString ? "true" : "false"; }

    private function cancelTarget():String {
        Player.CancelTarget();
        Player.CancelTargetSelf();
        Player.CancelAutoAttack();
        return "true";
    }

    // ---- Inventory / Bank / Shop ----

    private function hasItem(name:String, qty:int = 1):String { return Inventory.HasItem(name, qty) ? "true" : "false"; }
    private function hasTempItem(name:String, qty:int = 1):String { return TempInventory.ItemIsInTemp(name, qty <= 0 ? "*" : String(qty)) == Root.TrueString ? "true" : "false"; }
    private function hasBankItem(name:String, qty:int = 1):String { return Bank.HasItem(name, qty) ? "true" : "false"; }
    private function isEquipped(name:String):String { return Inventory.IsEquipped(name) ? "true" : "false"; }
    private function loadShop(shopId:int):String { Shops.Load(String(shopId)); return "true"; }
    private function isShopLoaded(shopId:int = 0):String { return Shops.IsShopLoaded(shopId) == Root.TrueString ? "true" : "false"; }
    private function buyItem(shopId:int, name:String):String { Shops.BuyItem(name); return "true"; }
    private function sellItem(name:String):String { Shops.SellItem(name); return "true"; }
    private function transferToBank(name:String):String { Bank.TransferToBank(name); return "true"; }
    private function transferToInv(name:String):String { Bank.TransferToInventory(name); return "true"; }
    private function openBank():String { Bank.Show(); return "true"; }
    private function getMapItem(id:int):String { Player.GetMapItem(String(id)); return "true"; }

    private function equipItem(name:String):String {
        var it:Object = Inventory.GetItemByName(name);
        if (it == null) return "false";
        Player.Equip(String(it.ItemID));
        return "true";
    }

    // ---- Quests ----

    private function loadQuest(id:int):String { Quests.Load(String(id)); return "true"; }
    private function isQuestLoaded(id:int):String { return Quests.IsLoaded(id) ? "true" : "false"; }
    private function isQuestCompletedOnce(id:int):String { return Quests.IsCompletedOnce(id) ? "true" : "false"; }
    private function acceptQuest(id:int):String { Quests.Accept(String(id)); return "true"; }
    private function completeQuest(id:int, itemId:int = -1, qty:int = 1):String { Quests.Complete(String(id), qty, String(itemId), "False"); return "true"; }
    private function isQuestInProgress(id:int):String { return Quests.IsInProgress(String(id)) == Root.TrueString ? "true" : "false"; }
    private function canCompleteQuest(id:int):String { return Quests.CanComplete(String(id)) == Root.TrueString ? "true" : "false"; }
}
}
