package grimlite.host {
import flash.display.DisplayObject;
import flash.display.Loader;
import flash.display.MovieClip;
import flash.display.Stage;
import flash.display.StageScaleMode;
import flash.events.ErrorEvent;
import flash.events.Event;
import flash.events.IOErrorEvent;
import flash.events.SecurityErrorEvent;
import flash.events.UncaughtErrorEvent;
import flash.external.ExternalInterface;
import flash.net.URLLoader;
import flash.net.URLRequest;
import flash.system.Security;
import flash.utils.setTimeout;

/**
 * Grimlite Ri host SWF: loads AQW's game client and bridges ExternalInterface calls to Rust.
 * Packet listening/manipulation/injection is handled directly in Ruffle's CaptureNavigator.
 */
public class Main extends MovieClip {
    private static const VERSION:int = 3;
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
    private var optSkipCutscenes:Boolean = false;
    private var optEnemyMagnet:Boolean = false;

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
        try {
            Security.allowDomain("*");
        } catch (e:Error) {}

        patchTrim();
        loaderInfo.uncaughtErrorEvents.addEventListener(UncaughtErrorEvent.UNCAUGHT_ERROR, onUncaughtError);

        expose("hostInfo", hostInfo);
        expose("loadClient", loadClient);
        expose("getGameObject", getGameObject);
        expose("setGameObject", setGameObject);
        expose("callGameFunction0", callGameFunction0);
        expose("callGameFunction", callGameFunction);
        expose("isNull", isNull);
        expose("isLoggedIn", isLoggedIn);
        expose("username", username);
        expose("getPlayerState", getPlayerState);

        // Options
        expose("setInfiniteRange", setInfiniteRange);
        expose("aggroMonsters", aggroMonsters);
        expose("setEnemyMagnet", setEnemyMagnet);
        expose("setLagKiller", setLagKiller);
        expose("setDisableAnimations", setDisableAnimations);
        expose("setSkipCutscenes", setSkipCutscenes);
        expose("setFPS", setFPS);
        expose("setWalkSpeed", setWalkSpeed);

        // Map
        expose("joinMap", joinMap);
        expose("moveToCell", moveToCell);
        expose("walkTo", walkTo);
        expose("setSpawnPoint", setSpawnPoint);
        expose("isMapLoading", isMapLoading);

        // Combat
        expose("attackMonster", attackMonster);
        expose("useSkill", useSkill);
        expose("cancelTarget", cancelTarget);
        expose("rest", rest);
        expose("isMonsterAvailable", isMonsterAvailable);
        expose("hasTarget", hasTarget);

        // Inventory / Bank / Shop / Drops
        expose("hasItem", hasItem);
        expose("hasTempItem", hasTempItem);
        expose("hasBankItem", hasBankItem);
        expose("equipItem", equipItem);
        expose("loadShop", loadShop);
        expose("buyItem", buyItem);
        expose("sellItem", sellItem);
        expose("transferToBank", transferToBank);
        expose("transferToInv", transferToInv);
        expose("getMapItem", getMapItem);

        // Quests
        expose("loadQuest", loadQuest);
        expose("acceptQuest", acceptQuest);
        expose("completeQuest", completeQuest);
        expose("isQuestInProgress", isQuestInProgress);
        expose("canCompleteQuest", canCompleteQuest);

        callHost("requestLoadGame", []);
        setTimeout(function():void {
            if (!loading && game == null) {
                loadClient();
            }
        }, 150);
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
        if (game == null || game.world == null) {
            return;
        }
        if (optSkipCutscenes) {
            try {
                if (game.mcExtSWF != null && game.mcExtSWF.numChildren > 0) {
                    game.mcExtSWF.removeChildren();
                    game.world.visible = true;
                }
            } catch (e:Error) {}
        }
        if (optEnemyMagnet) {
            try {
                var target:* = game.world.myAvatar.target;
                var me:* = game.world.myAvatar.pMC;
                if (target != null && target.pMC != null && me != null) {
                    target.pMC.x = me.x;
                    target.pMC.y = me.y;
                }
            } catch (e:Error) {}
        }
        if (optDisableAnimations) {
            try {
                var avatars:Object = game.world.avatars;
                for (var key:String in avatars) {
                    var av:* = avatars[key];
                    if (av != null && av.pMC != null && av.pMC.mcChar != null) {
                        av.pMC.mcChar.gotoAndStop("Idle");
                    }
                }
            } catch (e:Error) {}
        }
    }

    // ---- Batched Player State ----

    private function getPlayerState():String {
        var out:Object = {
            loggedIn: false,
            username: "",
            hp: 0,
            maxHp: 0,
            mp: 0,
            maxMp: 0,
            level: 0,
            gold: 0,
            map: "",
            cell: "",
            pad: "",
            state: 0,
            alive: false,
            targetName: "",
            targetHp: 0,
            targetMaxHp: 0
        };
        try {
            if (game != null && game.sfc != null && game.sfc.isConnected && game.world != null && game.world.myAvatar != null) {
                var av:* = game.world.myAvatar;
                var data:* = av.objData;
                out.loggedIn = true;
                if (data != null) {
                    out.username = String(data.strUsername || "");
                    out.hp = int(data.intHP);
                    out.maxHp = int(data.intHPMax);
                    out.mp = int(data.intMP);
                    out.maxMp = int(data.intMPMax);
                    out.level = int(data.intLevel);
                    out.gold = int(data.intGold);
                    out.state = int(data.intState);
                    out.alive = int(data.intHP) > 0 && int(data.intState) > 0;
                }
                out.map = String(game.world.strMapName || "");
                out.cell = String(game.world.strFrame || "");
                out.pad = String(game.world.strPad || "");
                if (av.target != null && av.target.objData != null) {
                    out.targetName = String(av.target.objData.strMonName || av.target.objData.strUsername || "");
                    out.targetHp = int(av.target.objData.intHP);
                    out.targetMaxHp = int(av.target.objData.intHPMax);
                }
            }
        } catch (e:Error) {}
        return JSON.stringify(out);
    }

    // ---- Grimlite-Li Options ----

    private function setInfiniteRange(on:* = true):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null && game.world.actions != null) {
                var range:int = (on == true || on == "true") ? 20000 : 300;
                var active:Array = game.world.actions.active;
                if (active != null) {
                    for (var i:int = 0; i < active.length; i++) {
                        if (active[i] != null) {
                            active[i].range = range;
                        }
                    }
                }
                var passive:Array = game.world.actions.passive;
                if (passive != null) {
                    for (var j:int = 0; j < passive.length; j++) {
                        if (passive[j] != null) {
                            passive[j].range = range;
                        }
                    }
                }
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function aggroMonsters():String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null && game.world.aggroAllMon is Function) {
                game.world.aggroAllMon();
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function setEnemyMagnet(on:* = true):String {
        optEnemyMagnet = (on == true || on == "true");
        return "true";
    }

    private function setLagKiller(on:* = true):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                var hide:Boolean = (on == true || on == "true");
                game.world.visible = !hide;
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function setDisableAnimations(on:* = true):String {
        optDisableAnimations = (on == true || on == "true");
        return "true";
    }

    private function setSkipCutscenes(on:* = true):String {
        optSkipCutscenes = (on == true || on == "true");
        return "true";
    }

    private function setFPS(fps:* = 30):String {
        var ok:String = "false";
        try {
            var rate:Number = Number(fps);
            if (stageRef != null && rate > 0) {
                stageRef.frameRate = rate;
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function setWalkSpeed(speed:* = 8):String {
        var ok:String = "false";
        try {
            var spd:int = int(speed);
            if (game != null && game.world != null && spd > 0) {
                game.world.WALKSPEED = spd;
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    // ---- Map ----

    private function joinMap(map:String, cell:String = "Enter", pad:String = "Spawn"):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                game.world.gotoTown(map, cell, pad);
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function moveToCell(cell:String, pad:String = "Spawn"):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                game.world.moveToCell(cell, pad);
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function walkTo(x:int, y:int):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null && game.world.myAvatar != null && game.world.myAvatar.pMC != null) {
                game.world.myAvatar.pMC.walkTo(x, y, game.world.WALKSPEED);
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function setSpawnPoint():String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                game.world.setSpawnPoint(game.world.strFrame, game.world.strPad);
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function isMapLoading():String {
        var res:String = "false";
        try {
            if (game != null && game.mcConnDetail != null) {
                res = game.mcConnDetail.stage != null ? "true" : "false";
            }
        } catch (e:Error) {}
        return res;
    }

    // ---- Combat ----

    private function findMonster(name:String):* {
        if (game == null || game.world == null || game.world.monsters == null) {
            return null;
        }
        var lower:String = (name != null) ? name.toLowerCase() : "*";
        var frame:String = String(game.world.strFrame || "");
        var mons:Array = game.world.monsters;
        for (var i:int = 0; i < mons.length; i++) {
            var m:* = mons[i];
            if (m == null || m.objData == null || m.dataLeaf == null) {
                continue;
            }
            if (String(m.dataLeaf.strFrame) != frame) {
                continue;
            }
            if (int(m.dataLeaf.intHP) <= 0 || int(m.dataLeaf.intState) <= 0) {
                continue;
            }
            var monName:String = String(m.objData.strMonName || "").toLowerCase();
            if (lower == "*" || lower.length == 0 || monName == lower || monName.indexOf(lower) >= 0) {
                return m;
            }
        }
        return null;
    }

    private function isMonsterAvailable(name:String = "*"):String {
        var res:String = "false";
        try {
            res = findMonster(name) != null ? "true" : "false";
        } catch (e:Error) {}
        return res;
    }

    private function attackMonster(name:String = "*"):String {
        var ok:String = "false";
        try {
            var m:* = findMonster(name);
            if (m != null && game.world != null) {
                game.world.setTarget(m);
                game.world.approachTarget();
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function useSkill(index:String):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null && game.world.actions != null) {
                var idx:int = int(index);
                var skill:* = game.world.actions.active[idx];
                if (skill != null && skill.isOK) {
                    game.world.testAction(skill);
                    ok = "true";
                }
            }
        } catch (e:Error) {}
        return ok;
    }

    private function cancelTarget():String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                game.world.cancelTarget();
                game.world.cancelAutoAttack();
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function rest(full:* = false):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                game.world.rest();
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function hasTarget():String {
        var res:String = "false";
        try {
            if (game != null && game.world != null && game.world.myAvatar != null) {
                var t:* = game.world.myAvatar.target;
                if (t != null && t.dataLeaf != null && int(t.dataLeaf.intHP) > 0) {
                    res = "true";
                }
            }
        } catch (e:Error) {}
        return res;
    }

    // ---- Inventory / Bank / Shop ----

    private static function checkListQty(list:Array, name:String, qty:int):Boolean {
        if (list == null || name == null) {
            return false;
        }
        var lower:String = name.toLowerCase();
        for (var i:int = 0; i < list.length; i++) {
            var it:* = list[i];
            if (it != null && String(it.sName || "").toLowerCase() == lower) {
                var have:int = int(it.iQty || 1);
                if (qty <= 0 || have >= qty) {
                    return true;
                }
            }
        }
        return false;
    }

    private function hasItem(name:String, qty:int = 1):String {
        var res:String = "false";
        try {
            if (game != null && game.world != null && game.world.myAvatar != null) {
                res = checkListQty(game.world.myAvatar.items, name, qty) ? "true" : "false";
            }
        } catch (e:Error) {}
        return res;
    }

    private function hasTempItem(name:String, qty:int = 1):String {
        var res:String = "false";
        try {
            if (game != null && game.world != null && game.world.myAvatar != null) {
                res = checkListQty(game.world.myAvatar.tempitems, name, qty) ? "true" : "false";
            }
        } catch (e:Error) {}
        return res;
    }

    private function hasBankItem(name:String, qty:int = 1):String {
        var res:String = "false";
        try {
            if (game != null && game.world != null && game.world.bankinfo != null) {
                res = checkListQty(game.world.bankinfo.items, name, qty) ? "true" : "false";
            }
        } catch (e:Error) {}
        return res;
    }

    private function equipItem(name:String):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null && game.world.myAvatar != null) {
                var lower:String = name.toLowerCase();
                var items:Array = game.world.myAvatar.items;
                for (var i:int = 0; i < items.length; i++) {
                    var it:* = items[i];
                    if (it != null && String(it.sName || "").toLowerCase() == lower) {
                        game.world.sendEquipItemRequest(it);
                        ok = "true";
                        break;
                    }
                }
            }
        } catch (e:Error) {}
        return ok;
    }

    private function loadShop(shopId:int):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                game.world.sendLoadShopRequest(shopId);
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function buyItem(shopId:int, name:String):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null && game.world.shopinfo != null) {
                var lower:String = name.toLowerCase();
                var items:Array = game.world.shopinfo.items;
                for (var i:int = 0; i < items.length; i++) {
                    var it:* = items[i];
                    if (it != null && String(it.sName || "").toLowerCase() == lower) {
                        game.world.sendBuyItemRequest(it);
                        ok = "true";
                        break;
                    }
                }
            }
        } catch (e:Error) {}
        return ok;
    }

    private function sellItem(name:String):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null && game.world.myAvatar != null) {
                var lower:String = name.toLowerCase();
                var items:Array = game.world.myAvatar.items;
                for (var i:int = 0; i < items.length; i++) {
                    var it:* = items[i];
                    if (it != null && String(it.sName || "").toLowerCase() == lower) {
                        game.world.sendSellItemRequest(it);
                        ok = "true";
                        break;
                    }
                }
            }
        } catch (e:Error) {}
        return ok;
    }

    private function transferToBank(name:String):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null && game.world.myAvatar != null) {
                var lower:String = name.toLowerCase();
                var items:Array = game.world.myAvatar.items;
                for (var i:int = 0; i < items.length; i++) {
                    var it:* = items[i];
                    if (it != null && String(it.sName || "").toLowerCase() == lower) {
                        game.world.sendBankFromInvRequest(it);
                        ok = "true";
                        break;
                    }
                }
            }
        } catch (e:Error) {}
        return ok;
    }

    private function transferToInv(name:String):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null && game.world.bankinfo != null) {
                var lower:String = name.toLowerCase();
                var items:Array = game.world.bankinfo.items;
                for (var i:int = 0; i < items.length; i++) {
                    var it:* = items[i];
                    if (it != null && String(it.sName || "").toLowerCase() == lower) {
                        game.world.sendBankToInvRequest(it);
                        ok = "true";
                        break;
                    }
                }
            }
        } catch (e:Error) {}
        return ok;
    }

    private function getMapItem(id:int):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                game.world.getMapItem(id);
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    // ---- Quests ----

    private function loadQuest(id:int):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                game.world.showQuests(String(id), "q");
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function acceptQuest(id:int):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                game.world.acceptQuest(id);
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function completeQuest(id:int, itemId:int = -1):String {
        var ok:String = "false";
        try {
            if (game != null && game.world != null) {
                game.world.tryQuestComplete(id, itemId, false);
                ok = "true";
            }
        } catch (e:Error) {}
        return ok;
    }

    private function isQuestInProgress(id:int):String {
        var res:String = "false";
        try {
            if (game != null && game.world != null) {
                res = game.world.isQuestInProgress(id) ? "true" : "false";
            }
        } catch (e:Error) {}
        return res;
    }

    private function canCompleteQuest(id:int):String {
        var res:String = "false";
        try {
            if (game != null && game.world != null) {
                res = game.world.canTurnInQuest(id) ? "true" : "false";
            }
        } catch (e:Error) {}
        return res;
    }

    // ---- Generic Game Bridge ----

    private function resolvePath(path:String):* {
        if (game == null || path == null || path.length == 0) {
            return null;
        }
        var cur:* = game;
        var parts:Array = path.split(".");
        for (var i:int = 0; i < parts.length; i++) {
            if (cur == null) {
                return null;
            }
            cur = cur[parts[i]];
        }
        return cur;
    }

    private function getGameObject(path:String):String {
        var res:String = "null";
        try {
            res = JSON.stringify(resolvePath(path));
        } catch (e:Error) {
            res = "null";
        }
        return res;
    }

    private function setGameObject(path:String, value:*):String {
        var ok:String = "false";
        try {
            if (game != null && path != null) {
                var parts:Array = path.split(".");
                var last:String = String(parts.pop());
                var cur:* = game;
                for (var i:int = 0; i < parts.length; i++) {
                    if (cur == null) {
                        return "false";
                    }
                    cur = cur[parts[i]];
                }
                if (cur != null) {
                    cur[last] = value;
                    ok = "true";
                }
            }
        } catch (e:Error) {
            ok = "false";
        }
        return ok;
    }

    private function callGameFunction0(path:String):String {
        return callGameFunction(path);
    }

    private function callGameFunction(path:String, ...args):String {
        var resStr:String = "null";
        try {
            if (game != null && path != null) {
                var parts:Array = path.split(".");
                var fnName:String = String(parts.pop());
                var target:* = game;
                for (var i:int = 0; i < parts.length; i++) {
                    if (target == null) {
                        return "null";
                    }
                    target = target[parts[i]];
                }
                if (target != null && (target[fnName] is Function)) {
                    var res:* = Function(target[fnName]).apply(target, args);
                    resStr = JSON.stringify(res);
                }
            }
        } catch (e:Error) {
            resStr = "null";
        }
        return resStr;
    }

    private function isNull(path:String):String {
        var res:String = "true";
        try {
            res = resolvePath(path) == null ? "true" : "false";
        } catch (e:Error) {
            res = "true";
        }
        return res;
    }

    private function isLoggedIn():String {
        var res:String = "false";
        try {
            res = (game != null && game.sfc != null && game.sfc.isConnected && game.world != null && game.world.myAvatar != null) ? "true" : "false";
        } catch (e:Error) {
            res = "false";
        }
        return res;
    }

    private function username():String {
        try {
            if (game != null && game.world != null && game.world.myAvatar != null && game.world.myAvatar.objData != null) {
                return String(game.world.myAvatar.objData.strUsername || "");
            }
        } catch (e:Error) {}
        return "";
    }
}
}
