package grimoire
{
	/**
	 * Minimal reflection bridge for reading/writing/invoking nested `Root.Game` paths.
	 */
	public class Caller
	{
		public static function getGameObject(path:String):String
		{
			try
			{
				return JSON.stringify(resolvePath(path));
			}
			catch (e:Error) {}
			return "null";
		}

		public static function setGameObject(path:String, value:*):String
		{
			try
			{
				if (Root.Game == null || path == null || path.length == 0)
				{
					return "false";
				}
				var parts:Array = path.split(".");
				var prop:String = String(parts.pop());
				var target:* = resolveParts(Root.Game, parts);
				if (target != null)
				{
					target[prop] = value;
					return "true";
				}
			}
			catch (e:Error) {}
			return "false";
		}

		public static function callGameFunction0(path:String):String
		{
			return callGameFunction(path);
		}

		public static function callGameFunction(path:String, ... args):String
		{
			try
			{
				if (Root.Game == null || path == null || path.length == 0)
				{
					return "null";
				}
				var parts:Array = path.split(".");
				var fnName:String = String(parts.pop());
				var target:* = resolveParts(Root.Game, parts);
				if (target != null && (target[fnName] is Function))
				{
					return JSON.stringify(Function(target[fnName]).apply(target, args));
				}
			}
			catch (e:Error) {}
			return "null";
		}

		public static function isNull(path:String):String
		{
			try
			{
				return resolvePath(path) == null ? "true" : "false";
			}
			catch (e:Error) {}
			return "true";
		}

		private static function resolvePath(path:String):*
		{
			if (Root.Game == null || path == null || path.length == 0)
			{
				return null;
			}
			return resolveParts(Root.Game, path.split("."));
		}

		private static function resolveParts(root:*, parts:Array):*
		{
			var cur:* = root;
			for (var i:int = 0; i < parts.length; i++)
			{
				if (cur == null)
				{
					return null;
				}
				cur = cur[parts[i]];
			}
			return cur;
		}
	}
}
