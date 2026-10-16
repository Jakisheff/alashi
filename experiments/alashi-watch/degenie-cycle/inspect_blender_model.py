import bpy,json
print("SCENE_INFO",json.dumps({"version":bpy.app.version_string,"objects":[(o.name,o.type) for o in bpy.data.objects],"bones":list(bpy.data.objects["DeskGenie-rig"].pose.bones.keys()),"render":bpy.context.scene.render.engine}))
