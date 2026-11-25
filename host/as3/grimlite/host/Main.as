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
 * Packet listening/manipulation is handled directly in Ruffle's CaptureNavigator (no sfc.debug).
 */
public class Main extends MovieClip {
    private static const VERSION:int = 1;
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

        callHost("requestLoadGame", []);
        // Auto-load AQW client immediately if host hasn't triggered loadClient yet.
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
        callHost("loaded", []);
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
