import bpy
from pathlib import Path
from mathutils import Vector


ROOT = Path(__file__).resolve().parent
ASSETS = ROOT.parent
SOURCE_BLEND = ROOT / "butter-paper-glass-document.blend"
OUTPUTS = {
    ("stable", "light"): ASSETS / "butter-paper-icon.png",
    ("stable", "dark"): ASSETS / "butter-paper-icon-dark.png",
    ("beta", "light"): ASSETS / "butter-paper-icon-beta.png",
    ("beta", "dark"): ASSETS / "butter-paper-icon-beta-dark.png",
}


def set_input(node, names, value):
    for name in names:
        socket = node.inputs.get(name)
        if socket is not None:
            socket.default_value = value
            return


def set_material_color(name, color):
    material = bpy.data.materials.get(name)
    if material is None:
        raise RuntimeError(f"Missing material: {name}")
    bsdf = material.node_tree.nodes.get("Principled BSDF")
    set_input(bsdf, ["Base Color"], (*color, 1.0))


def set_glass_absorption(name, color, density):
    material = bpy.data.materials.get(name)
    if material is None:
        raise RuntimeError(f"Missing material: {name}")
    volume = next(
        (node for node in material.node_tree.nodes if node.bl_idname == "ShaderNodeVolumeAbsorption"),
        None,
    )
    if volume is None:
        raise RuntimeError(f"Missing volume absorption node: {name}")
    volume.inputs["Color"].default_value = (*color, 1.0)
    volume.inputs["Density"].default_value = density


def set_material_emission(name, color, strength):
    material = bpy.data.materials.get(name)
    if material is None:
        raise RuntimeError(f"Missing material: {name}")
    bsdf = material.node_tree.nodes.get("Principled BSDF")
    set_input(bsdf, ["Emission Color", "Emission"], (*color, 1.0))
    set_input(bsdf, ["Emission Strength"], strength)


def set_annotation_material(channel):
    material = bpy.data.materials.get("Vibrant ruby annotation glass")
    if material is None:
        raise RuntimeError("Missing vibrant ruby annotation material")
    if channel == "stable":
        surface = (0.96, 0.004, 0.012)
        emission = (0.34, 0.001, 0.003)
        absorption = (0.98, 0.001, 0.004)
    else:
        surface = (0.30, 0.025, 0.92)
        emission = (0.095, 0.006, 0.34)
        absorption = (0.38, 0.015, 0.98)
    bsdf = material.node_tree.nodes.get("Principled BSDF")
    set_input(bsdf, ["Base Color"], (*surface, 1.0))
    set_input(bsdf, ["Emission Color", "Emission"], (*emission, 1.0))
    set_input(bsdf, ["Emission Strength"], 0.18)
    set_glass_absorption(
        "Vibrant ruby annotation glass",
        absorption,
        0.024,
    )


def aim_at(obj, target):
    direction = Vector(target) - obj.location
    obj.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()


def apply_theme(theme, channel):
    if theme == "light":
        colors = {
            "Natural white document paper": (0.92, 0.91, 0.87),
            "Embedded charcoal heavy ink": (0.010, 0.013, 0.017),
            "Embedded charcoal fine ink": (0.045, 0.052, 0.058),
            "Embedded grey construction ink": (0.23, 0.25, 0.25),
            "Document image background": (0.43, 0.43, 0.43),
            "Document image middle tone": (0.59, 0.59, 0.59),
            "Document image dark tone": (0.12, 0.12, 0.12),
            "Document image light tone": (0.78, 0.78, 0.78),
            "Dark neutral preview surface": (0.92, 0.91, 0.87),
        }
        world_color = (0.075, 0.070, 0.060)
        trace_surface = (0.98, 0.92, 0.62)
        trace_absorption = (0.92, 0.70, 0.22)
        trace_density = 0.0055
        ink_emission = 0.0
        light_energy_scale = 1.0
    else:
        colors = {
            "Natural white document paper": (0.001, 0.002, 0.004),
            "Embedded charcoal heavy ink": (1.0, 1.0, 1.0),
            "Embedded charcoal fine ink": (0.93, 0.95, 0.98),
            "Embedded grey construction ink": (0.72, 0.76, 0.82),
            "Document image background": (0.08, 0.09, 0.11),
            "Document image middle tone": (0.40, 0.43, 0.48),
            "Document image dark tone": (0.72, 0.76, 0.82),
            "Document image light tone": (0.88, 0.90, 0.94),
            "Dark neutral preview surface": (0.001, 0.002, 0.004),
        }
        world_color = (0.0005, 0.0008, 0.0015)
        trace_surface = (0.96, 0.95, 0.90)
        trace_absorption = (1.0, 0.88, 0.54)
        trace_density = 0.0010
        ink_emission = 0.24
        light_energy_scale = 0.28

    for name, color in colors.items():
        set_material_color(name, color)
    set_material_color("Transparent yellow trace glass", trace_surface)
    set_glass_absorption(
        "Transparent yellow trace glass",
        trace_absorption,
        trace_density,
    )
    for name, strength in (
        ("Embedded charcoal heavy ink", ink_emission),
        ("Embedded charcoal fine ink", ink_emission * 0.78),
        ("Embedded grey construction ink", ink_emission * 0.48),
    ):
        set_material_emission(name, colors[name], strength)
    set_annotation_material(channel)
    if theme == "dark":
        for light in bpy.data.lights:
            light.energy *= light_energy_scale
    bpy.context.scene.world.color = world_color


def configure_scene():
    camera = bpy.context.scene.camera
    camera.data.type = "ORTHO"
    camera.data.ortho_scale = 5.85
    camera.location = (0.0, -0.12, 14.0)
    aim_at(camera, (0.0, 0.0, 0.34))

    document_content = bpy.data.objects.get("Expanded monochrome document content")
    if document_content is None:
        raise RuntimeError("Missing expanded document-content assembly")
    document_content.scale = (1.08, 1.08, 1.0)

    scene = bpy.context.scene
    scene.render.resolution_x = 1024
    scene.render.resolution_y = 1024
    scene.render.resolution_percentage = 100
    scene.cycles.samples = 56


def render_variant(channel, theme, output):
    bpy.ops.wm.open_mainfile(filepath=str(SOURCE_BLEND))
    configure_scene()
    apply_theme(theme, channel)
    scene = bpy.context.scene
    scene.render.film_transparent = False
    scene.render.filepath = str(output)
    bpy.ops.render.render(write_still=True)


def main():
    for (channel, theme), output in OUTPUTS.items():
        render_variant(channel, theme, output)


if __name__ == "__main__":
    main()
