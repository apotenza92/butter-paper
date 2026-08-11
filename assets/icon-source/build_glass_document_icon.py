import bpy
import math
from pathlib import Path
from mathutils import Vector


ROOT = Path(__file__).resolve().parent
ROOT.mkdir(parents=True, exist_ok=True)
OUTPUT_BLEND = ROOT / "butter-paper-glass-document.blend"
OUTPUT_PREVIEW = ROOT / "butter-paper-glass-drawing-slab-preview.png"
OUTPUT_TRANSPARENT = ROOT / "butter-paper-glass-drawing-slab-transparent.png"


def set_input(node, names, value):
    for name in names:
        socket = node.inputs.get(name)
        if socket is not None:
            socket.default_value = value
            return


def material(name, base, roughness=0.25, metallic=0.0, transmission=0.0,
             ior=1.47, coat=0.2, emission=None):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    set_input(bsdf, ["Base Color"], (*base, 1.0))
    set_input(bsdf, ["Roughness"], roughness)
    set_input(bsdf, ["Metallic"], metallic)
    set_input(bsdf, ["Transmission Weight", "Transmission"], transmission)
    set_input(bsdf, ["IOR"], ior)
    set_input(bsdf, ["Coat Weight", "Clearcoat"], coat)
    set_input(bsdf, ["Coat Roughness", "Clearcoat Roughness"], max(0.02, roughness * 0.38))
    if emission:
        set_input(bsdf, ["Emission Color", "Emission"], (*emission, 1.0))
        set_input(bsdf, ["Emission Strength"], 0.035)
    return mat


def glass(name, surface, absorption, density, roughness=0.045, ior=1.50):
    mat = material(name, surface, roughness=roughness, transmission=0.96, ior=ior, coat=0.52)
    nodes = mat.node_tree.nodes
    output = nodes.get("Material Output")
    volume = nodes.new("ShaderNodeVolumeAbsorption")
    volume.inputs["Color"].default_value = (*absorption, 1.0)
    volume.inputs["Density"].default_value = density
    mat.node_tree.links.new(volume.outputs["Volume"], output.inputs["Volume"])
    return mat


def paper_material(name, base):
    mat = material(name, base, roughness=0.62, transmission=0.0, ior=1.40, coat=0.025)
    nodes = mat.node_tree.nodes
    links = mat.node_tree.links
    bsdf = nodes.get("Principled BSDF")
    noise = nodes.new("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = 150.0
    noise.inputs["Detail"].default_value = 2.2
    noise.inputs["Roughness"].default_value = 0.74
    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.12
    bump.inputs["Distance"].default_value = 0.012
    links.new(noise.outputs["Fac"], bump.inputs["Height"])
    links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    return mat


def leather_material(name, base):
    mat = material(name, base, roughness=0.42, transmission=0.0, ior=1.45, coat=0.14)
    nodes = mat.node_tree.nodes
    links = mat.node_tree.links
    bsdf = nodes.get("Principled BSDF")
    noise = nodes.new("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = 68.0
    noise.inputs["Detail"].default_value = 4.0
    noise.inputs["Roughness"].default_value = 0.72
    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.20
    bump.inputs["Distance"].default_value = 0.018
    links.new(noise.outputs["Fac"], bump.inputs["Height"])
    links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    return mat


def assign(obj, mat):
    obj.data.materials.clear()
    obj.data.materials.append(mat)


def rounded_box(name, location, scale, bevel, mat, rotation=0.0):
    bpy.ops.mesh.primitive_cube_add(location=location)
    obj = bpy.context.object
    obj.name = name
    obj.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    modifier = obj.modifiers.new("Continuous rounded edge", "BEVEL")
    modifier.width = bevel
    modifier.segments = 8
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=modifier.name)
    obj.rotation_euler[2] = rotation
    bpy.ops.object.shade_smooth_by_angle()
    assign(obj, mat)
    return obj


def superellipse_slab(
    name,
    location,
    half_size,
    half_depth,
    exponent,
    mat,
    segments=192,
    bevel_width=0.045,
):
    perimeter = []
    for index in range(segments):
        angle = 2.0 * math.pi * index / segments
        cosine = math.cos(angle)
        sine = math.sin(angle)
        perimeter.append((
            half_size[0] * math.copysign(abs(cosine) ** (2.0 / exponent), cosine),
            half_size[1] * math.copysign(abs(sine) ** (2.0 / exponent), sine),
        ))

    top = [(x, y, half_depth) for x, y in perimeter]
    bottom = [(x, y, -half_depth) for x, y in perimeter]
    verts = top + bottom
    count = len(perimeter)
    faces = [tuple(range(count)), tuple(reversed(range(count, count * 2)))]
    for index in range(count):
        nxt = (index + 1) % count
        faces.append((index, nxt, count + nxt, count + index))

    mesh = bpy.data.meshes.new(name + " mesh")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    obj.location = location
    assign(obj, mat)
    bevel = obj.modifiers.new("Soft superellipse slab edge", "BEVEL")
    bevel.width = bevel_width
    bevel.segments = 8
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=bevel.name)
    bpy.ops.object.shade_smooth_by_angle()
    return obj


def build_regular_paper(name, width, height, mat, nx=112, ny=108):
    half_w = width / 2
    half_h = height / 2
    verts = []
    faces = []

    def smoothstep(value):
        value = max(0.0, min(1.0, value))
        return value * value * (3.0 - 2.0 * value)

    for row in range(ny + 1):
        y = -half_h + height * row / ny
        for col in range(nx + 1):
            x = -half_w + width * col / nx
            z = 0.018
            corner_x = smoothstep((abs(x) - (half_w - 0.72)) / 0.72)
            corner_y = smoothstep((abs(y) - (half_h - 0.72)) / 0.72)
            corner = corner_x * corner_y
            if x >= 0 and y >= 0:
                z += 0.34 * corner
            elif x < 0 and y >= 0:
                z += 0.26 * corner
            elif x < 0 and y < 0:
                z += 0.12 * corner
            else:
                z += 0.0 * corner
            verts.append((x, y, z))

    stride = nx + 1
    for row in range(ny):
        for col in range(nx):
            index = row * stride + col
            faces.append((index, index + 1, index + stride + 1, index + stride))
    mesh = bpy.data.meshes.new(name + " mesh")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    assign(obj, mat)
    for polygon in obj.data.polygons:
        polygon.use_smooth = True
    solidify = obj.modifiers.new("Real paper thickness", "SOLIDIFY")
    solidify.thickness = 0.038
    solidify.offset = -1.0
    return obj


def add_text(name, body, location, size, mat):
    bpy.ops.object.text_add(location=location)
    obj = bpy.context.object
    obj.name = name
    obj.data.body = body
    obj.data.align_x = "LEFT"
    obj.data.align_y = "CENTER"
    obj.data.size = size
    obj.data.extrude = 0.004
    obj.data.bevel_depth = 0.0015
    obj.data.bevel_resolution = 2
    assign(obj, mat)
    return obj


def flat_polygon(name, points, z, thickness, mat):
    top = [(x, y, z + thickness / 2) for x, y in points]
    bottom = [(x, y, z - thickness / 2) for x, y in points]
    verts = top + bottom
    count = len(points)
    faces = [tuple(range(count)), tuple(reversed(range(count, count * 2)))]
    for index in range(count):
        nxt = (index + 1) % count
        faces.append((index, nxt, count + nxt, count + index))
    mesh = bpy.data.meshes.new(name + " mesh")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    assign(obj, mat)
    return obj


def frustum_ring(name, z0, z1, outer_bottom, outer_top, inner_bottom, inner_top,
                 segments, mat, center=(0.0, 0.0), bevel=0.0):
    verts = []
    cx, cy = center
    for z, radius in ((z0, outer_bottom), (z1, outer_top),
                      (z0, inner_bottom), (z1, inner_top)):
        for index in range(segments):
            angle = 2.0 * math.pi * index / segments
            verts.append((cx + radius * math.cos(angle), cy + radius * math.sin(angle), z))
    ob, ot, ib, it = 0, segments, segments * 2, segments * 3
    faces = []
    for index in range(segments):
        nxt = (index + 1) % segments
        faces.append((ob + index, ob + nxt, ot + nxt, ot + index))
        faces.append((ib + nxt, ib + index, it + index, it + nxt))
        faces.append((ot + index, ot + nxt, it + nxt, it + index))
        faces.append((ob + nxt, ob + index, ib + index, ib + nxt))
    mesh = bpy.data.meshes.new(name + " mesh")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    assign(obj, mat)
    if bevel:
        modifier = obj.modifiers.new("Rounded optical edges", "BEVEL")
        modifier.width = bevel
        modifier.segments = 5
        bpy.context.view_layer.objects.active = obj
        bpy.ops.object.modifier_apply(modifier=modifier.name)
    for polygon in obj.data.polygons:
        polygon.use_smooth = True
    return obj


def torus(name, location, major_radius, minor_radius, mat, scale_z=1.0):
    bpy.ops.mesh.primitive_torus_add(major_radius=major_radius, minor_radius=minor_radius,
                                    major_segments=128, minor_segments=24, location=location)
    obj = bpy.context.object
    obj.name = name
    obj.scale.z = scale_z
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    bpy.ops.object.shade_smooth()
    assign(obj, mat)
    return obj


def line(name, x, y, length, width, angle, z, mat):
    return rounded_box(name, (x, y, z), (length / 2, width / 2, 0.010),
                       min(width * 0.46, 0.016), mat, angle)


def curve_stroke(name, points, bevel_depth, mat):
    curve = bpy.data.curves.new(name + " curve", "CURVE")
    curve.dimensions = "3D"
    curve.resolution_u = 20
    curve.bevel_depth = bevel_depth
    curve.bevel_resolution = 5
    spline = curve.splines.new("BEZIER")
    spline.bezier_points.add(len(points) - 1)
    for point, coordinate in zip(spline.bezier_points, points):
        point.co = coordinate
        point.handle_left_type = "AUTO"
        point.handle_right_type = "AUTO"
    obj = bpy.data.objects.new(name, curve)
    bpy.context.collection.objects.link(obj)
    assign(obj, mat)
    return obj


def filled_arrow(name, tail, tip, shaft_width, head_width, head_length, z, thickness, mat):
    tail_v = Vector(tail)
    tip_v = Vector(tip)
    direction = (tip_v - tail_v).normalized()
    perpendicular = Vector((-direction.y, direction.x))
    neck = tip_v - direction * head_length
    outline = [
        tail_v + perpendicular * shaft_width / 2,
        neck + perpendicular * shaft_width / 2,
        neck + perpendicular * head_width / 2,
        tip_v,
        neck - perpendicular * head_width / 2,
        neck - perpendicular * shaft_width / 2,
        tail_v - perpendicular * shaft_width / 2,
    ]
    verts = [(point.x, point.y, z + thickness / 2) for point in outline]
    verts += [(point.x, point.y, z - thickness / 2) for point in outline]
    count = len(outline)
    faces = [tuple(range(count)), tuple(reversed(range(count, count * 2)))]
    for index in range(count):
        nxt = (index + 1) % count
        faces.append((index, nxt, count + nxt, count + index))
    mesh = bpy.data.meshes.new(name + " mesh")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    assign(obj, mat)
    bevel = obj.modifiers.new("Rounded filled arrow edge", "BEVEL")
    bevel.width = 0.035
    bevel.segments = 4
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=bevel.name)
    bpy.ops.object.shade_smooth_by_angle()
    return obj


def curled_page_edge(name, x0, x1, y_inner, width, z_inner, rise, mat, u_steps=40, v_steps=24):
    centre = (x0 + x1) / 2
    verts = []
    faces = []
    for row in range(v_steps + 1):
        v = row / v_steps
        theta = v * math.pi / 2
        y = y_inner - width * math.sin(theta)
        z = z_inner + rise * (1.0 - math.cos(theta))
        taper = 1.0 - 0.34 * v
        for col in range(u_steps + 1):
            base_x = x0 + (x1 - x0) * col / u_steps
            x = centre + (base_x - centre) * taper
            verts.append((x, y, z))
    stride = u_steps + 1
    for row in range(v_steps):
        for col in range(u_steps):
            index = row * stride + col
            faces.append((index, index + 1, index + stride + 1, index + stride))
    mesh = bpy.data.meshes.new(name + " mesh")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    assign(obj, mat)
    for polygon in obj.data.polygons:
        polygon.use_smooth = True
    solidify = obj.modifiers.new("Thin curled page edge", "SOLIDIFY")
    solidify.thickness = 0.035
    solidify.offset = -0.5
    return obj


def ellipse(name, location, scale, bevel_depth, mat, angle=0.0):
    bpy.ops.curve.primitive_bezier_circle_add(radius=1.0, location=location)
    obj = bpy.context.object
    obj.name = name
    obj.scale = (scale[0], scale[1], 1.0)
    obj.rotation_euler[2] = angle
    obj.data.bevel_depth = bevel_depth
    obj.data.bevel_resolution = 6
    obj.data.resolution_u = 24
    assign(obj, mat)
    return obj


def revision_cloud(
    name,
    location,
    bevel_depth,
    mat,
    lobe_radius,
    horizontal_lobes,
    vertical_lobes,
    arc_steps=10,
):
    """Build a drafting-style revision cloud from equal semicircular arcs."""
    half_width = lobe_radius * horizontal_lobes
    half_height = lobe_radius * vertical_lobes
    left = location[0] - half_width
    right = location[0] + half_width
    bottom = location[1] - half_height
    top = location[1] + half_height
    points = []

    def append_arc(center_x, center_y, start_angle, end_angle):
        for step in range(arc_steps + 1):
            if points and step == 0:
                continue
            angle = start_angle + (end_angle - start_angle) * step / arc_steps
            points.append((
                center_x + lobe_radius * math.cos(angle),
                center_y + lobe_radius * math.sin(angle),
                location[2],
            ))

    # Walk clockwise. Each side uses the same radius, diameter, and sampling,
    # so every lobe has identical geometry and adjacent lobes meet cleanly.
    for index in range(horizontal_lobes):
        center_x = left + lobe_radius + 2.0 * lobe_radius * index
        append_arc(center_x, top, math.pi, 0.0)
    for index in range(vertical_lobes):
        center_y = top - lobe_radius - 2.0 * lobe_radius * index
        append_arc(right, center_y, math.pi / 2.0, -math.pi / 2.0)
    for index in range(horizontal_lobes):
        center_x = right - lobe_radius - 2.0 * lobe_radius * index
        append_arc(center_x, bottom, 0.0, -math.pi)
    for index in range(vertical_lobes):
        center_y = bottom + lobe_radius + 2.0 * lobe_radius * index
        append_arc(left, center_y, -math.pi / 2.0, -3.0 * math.pi / 2.0)

    curve = bpy.data.curves.new(name + " curve", "CURVE")
    curve.dimensions = "3D"
    curve.bevel_depth = bevel_depth
    curve.bevel_resolution = 6
    spline = curve.splines.new("POLY")
    spline.points.add(len(points) - 1)
    for point, coordinate in zip(spline.points, points):
        point.co = (*coordinate, 1.0)
    spline.use_cyclic_u = True
    obj = bpy.data.objects.new(name, curve)
    bpy.context.collection.objects.link(obj)
    assign(obj, mat)
    return obj


def superellipse_outline(name, location, half_size, exponent, bevel_depth, mat, points=64):
    curve = bpy.data.curves.new(name + " curve", "CURVE")
    curve.dimensions = "3D"
    curve.resolution_u = 16
    curve.bevel_depth = bevel_depth
    curve.bevel_resolution = 6
    spline = curve.splines.new("BEZIER")
    spline.bezier_points.add(points - 1)
    for index, point in enumerate(spline.bezier_points):
        angle = 2.0 * math.pi * index / points
        cosine = math.cos(angle)
        sine = math.sin(angle)
        point.co = (
            location[0] + half_size[0] * math.copysign(abs(cosine) ** (2.0 / exponent), cosine),
            location[1] + half_size[1] * math.copysign(abs(sine) ** (2.0 / exponent), sine),
            location[2],
        )
        point.handle_left_type = "AUTO"
        point.handle_right_type = "AUTO"
    spline.use_cyclic_u = True
    obj = bpy.data.objects.new(name, curve)
    bpy.context.collection.objects.link(obj)
    assign(obj, mat)
    return obj


def aim_at(obj, target):
    direction = Vector(target) - obj.location
    obj.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()


def add_area(name, location, energy, size, color, target=(0, 0, 0)):
    bpy.ops.object.light_add(type="AREA", location=location)
    light = bpy.context.object
    light.name = name
    light.data.energy = energy
    light.data.shape = "RECTANGLE"
    light.data.size = size
    light.data.size_y = max(0.20, size * 0.06)
    light.data.specular_factor = 0.0
    light.data.transmission_factor = 0.72
    light.visible_glossy = False
    light.visible_transmission = True
    light.data.color = color
    aim_at(light, target)
    return light


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)

    white_paper = paper_material("Natural white document paper", (0.91, 0.90, 0.86))
    trace_glass = glass(
        "Transparent yellow trace glass",
        (0.98, 0.92, 0.62),
        (0.92, 0.70, 0.22),
        density=0.0055,
        roughness=0.030,
        ior=1.52,
    )
    trace_bsdf = trace_glass.node_tree.nodes.get("Principled BSDF")
    set_input(trace_bsdf, ["Coat Weight", "Clearcoat"], 0.10)
    set_input(trace_bsdf, ["IOR Level", "Specular IOR Level"], 0.14)
    heavy_ink = material("Embedded charcoal heavy ink", (0.010, 0.013, 0.017), roughness=0.34, coat=0.08)
    fine_ink = material("Embedded charcoal fine ink", (0.045, 0.052, 0.058), roughness=0.40, coat=0.05)
    construction = material("Embedded grey construction ink", (0.23, 0.25, 0.25), roughness=0.44, coat=0.03)
    image_background = material("Document image background", (0.43, 0.43, 0.43), roughness=0.46, coat=0.03)
    image_mid = material("Document image middle tone", (0.59, 0.59, 0.59), roughness=0.48, coat=0.02)
    image_dark = material("Document image dark tone", (0.12, 0.12, 0.12), roughness=0.44, coat=0.03)
    image_light = material("Document image light tone", (0.78, 0.78, 0.78), roughness=0.42, coat=0.04)
    red = glass(
        "Vibrant ruby annotation glass",
        (0.96, 0.004, 0.012),
        (0.98, 0.001, 0.004),
        density=0.024,
        roughness=0.055,
        ior=1.48,
    )
    red_bsdf = red.node_tree.nodes.get("Principled BSDF")
    set_input(red_bsdf, ["Coat Weight", "Clearcoat"], 0.12)
    set_input(red_bsdf, ["IOR Level", "Specular IOR Level"], 0.16)
    set_input(red_bsdf, ["Emission Color", "Emission"], (0.34, 0.001, 0.003, 1.0))
    set_input(red_bsdf, ["Emission Strength"], 0.18)

    build_regular_paper("Regular white document with lifted corners", 6.20, 5.95, white_paper)
    trace_panel = superellipse_slab(
        "Raised transparent yellow trace layer",
        (0, 0, 0.420),
        (3.10, 2.975),
        0.320,
        5.0,
        trace_glass,
        bevel_width=0.110,
    )

    drawing_z = 0.052
    draw = lambda name, x, y, length, width=0.032, angle=0.0, mat=fine_ink: line(
        name, x, y, length, width, angle, drawing_z, mat
    )
    content_existing_names = set(bpy.data.objects.keys())

    # Abstract typographic rhythm keeps the page document-like without using words.
    draw("Title block one", -0.37, 2.10, 2.10, 0.15, 0.0, heavy_ink)
    draw("Title block two", 1.14, 2.10, 0.56, 0.15, 0.0, heavy_ink)
    draw("Subtitle block one", -0.36, 1.79, 1.76, 0.065, 0.0, fine_ink)
    draw("Subtitle block two", 0.95, 1.79, 0.58, 0.065, 0.0, fine_ink)
    draw("Header rule", 0.0, 1.35, 4.30, 0.030, 0.0, construction)

    # Image frame and a restrained landscape/photo motif.
    rounded_box("Indicative document image", (-1.15, 0.32, 0.048), (0.92, 0.68, 0.012),
                0.075, image_background)
    flat_polygon("Image distant hill",
                 [(-2.02, -0.07), (-1.48, 0.43), (-0.98, 0.06), (-0.42, 0.52),
                  (-0.23, -0.23), (-2.02, -0.23)],
                 0.067, 0.015, image_mid)
    flat_polygon("Image foreground hill",
                 [(-2.02, -0.28), (-1.30, 0.18), (-0.76, -0.10), (-0.23, 0.24),
                  (-0.23, -0.35), (-2.02, -0.35)],
                 0.077, 0.016, image_dark)
    bpy.ops.mesh.primitive_cylinder_add(vertices=64, radius=0.15, depth=0.018,
                                        location=(-1.66, 0.66, 0.080))
    image_sun = bpy.context.object
    image_sun.name = "Image sun detail"
    assign(image_sun, image_light)
    # Heading and paragraph blocks remain recognisable at icon scale.
    draw("Body heading block one", 0.61, 0.96, 0.72, 0.095, 0.0, heavy_ink)
    draw("Body heading block two", 1.43, 0.96, 0.62, 0.095, 0.0, heavy_ink)
    for index, (x, y, length) in enumerate([
        (1.20, 0.62, 1.90), (1.20, 0.40, 1.90), (1.20, 0.18, 1.90),
        (1.20, -0.04, 1.90), (0.975, -0.26, 1.45),
    ], 1):
        draw(f"Body text line {index}", x, y, length, 0.030, 0.0, fine_ink)
    draw("Second heading block one", -1.74, -1.08, 0.84, 0.090, 0.0, heavy_ink)
    draw("Second heading block two", -0.78, -1.08, 0.72, 0.090, 0.0, heavy_ink)
    for index, (x, y, length) in enumerate([
        (-1.10, -1.36, 2.10), (-1.10, -1.60, 2.10),
        (-1.10, -1.84, 2.10), (-1.35, -2.08, 1.60),
    ], 1):
        draw(f"Summary text line {index}", x, y, length, 0.028, 0.0, fine_ink)

    # Use the white sheet as a real document canvas. Expand every printed
    # element together so the layout reaches close to the paper margins.
    bpy.ops.object.empty_add(type="PLAIN_AXES", location=(0, 0, 0))
    document_content = bpy.context.object
    document_content.name = "Expanded monochrome document content"
    for member in tuple(bpy.data.objects):
        if member.name not in content_existing_names and member is not document_content:
            member.parent = document_content
    document_content.scale = (1.08, 1.08, 1.0)

    # Centre a softer revision cloud on the scaled document image. Build a
    # separate bottom-right callout whose arrow grows directly from the top
    # middle of the note box and points into the upper-right paragraph.
    cloud_center = (-1.15 * 1.08, 0.32 * 1.08)
    text_box_center = (1.30, -1.74)
    text_box_half_size = (0.90, 0.44)
    text_box_stroke_radius = 0.055
    arrow_vertical_offset = text_box_stroke_radius * 2.0
    arrow_tail_y = -1.62 + text_box_half_size[1] + arrow_vertical_offset
    arrow_tip_y = -0.42 + arrow_vertical_offset

    revision_cloud(
        "Oxblood revision cloud",
        (*cloud_center, 0.800),
        0.034,
        red,
        lobe_radius=0.11,
        horizontal_lobes=10,
        vertical_lobes=8,
        arc_steps=12,
    )
    filled_arrow(
        "Filled oxblood review arrow",
        tail=(
            text_box_center[0],
            arrow_tail_y,
        ),
        tip=(text_box_center[0], arrow_tip_y),
        shaft_width=0.18,
        head_width=0.58,
        head_length=0.50,
        z=0.860,
        thickness=0.060,
        mat=red,
    )
    markup_text_box = superellipse_outline(
        "Transparent ruby markup text box outline",
        (*text_box_center, 0.860),
        text_box_half_size,
        5.0,
        text_box_stroke_radius,
        red,
        points=64,
    )
    note_line_left = text_box_center[0] - 0.72
    note_line_y_offsets = (0.23, 0.00, -0.23)
    markup_note_lines = (
        rounded_box("Ruby markup note line 1", (note_line_left + 0.70, text_box_center[1] + note_line_y_offsets[0], 0.860),
                    (0.70, 0.055, 0.030), 0.042, red),
        rounded_box("Ruby markup note line 2", (note_line_left + 0.54, text_box_center[1] + note_line_y_offsets[1], 0.860),
                    (0.54, 0.055, 0.030), 0.042, red),
        rounded_box("Ruby markup note line 3", (note_line_left + 0.64, text_box_center[1] + note_line_y_offsets[2], 0.860),
                    (0.64, 0.055, 0.030), 0.042, red),
    )

    # Let the trace glass define the full icon footprint. It matches the white
    # document below and uses the same superellipse corner model as the macOS
    # icon preflight mask. The ruby glass markups remain attached to it.
    bpy.ops.object.empty_add(type="PLAIN_AXES", location=(0, 0, 0))
    trace_assembly = bpy.context.object
    trace_assembly.name = "Aligned trace annotation assembly"
    trace_members = (
        trace_panel,
        bpy.data.objects.get("Oxblood revision cloud"),
        bpy.data.objects.get("Filled oxblood review arrow"),
        markup_text_box,
        *markup_note_lines,
    )
    for member in trace_members:
        if member is not None:
            member.parent = trace_assembly
    trace_assembly.rotation_euler[2] = 0.0

    bpy.ops.object.camera_add(location=(-3.85, -6.68, 11.90))
    camera = bpy.context.object
    camera.name = "Glass slab icon camera"
    camera.data.type = "PERSP"
    camera.data.lens = 55
    camera.data.sensor_width = 36
    aim_at(camera, (0.0, 0.0, 0.31))
    bpy.context.scene.camera = camera

    add_area("Large warm key", (-5.2, 4.8, 9.4), 920, 5.0, (1.0, 0.78, 0.50), (0, 0.8, 0.1))
    add_area("Cool glass fill", (5.2, 4.0, 7.4), 620, 4.6, (0.58, 0.74, 1.0), (0, 0.7, 0.1))
    add_area("Top edge strip", (0.0, 5.2, 10.0), 500, 3.8, (1.0, 0.91, 0.72), (0, 1.0, 0.2))

    backdrop_mat = material("Dark neutral preview surface", (0.018, 0.024, 0.034), roughness=0.78, coat=0.01)
    bpy.ops.mesh.primitive_plane_add(size=30, location=(0, 0, -0.48))
    backdrop = bpy.context.object
    backdrop.name = "Preview shadow surface"
    assign(backdrop, backdrop_mat)

    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 72
    scene.cycles.use_denoising = True
    scene.cycles.max_bounces = 10
    scene.cycles.transmission_bounces = 8
    scene.cycles.transparent_max_bounces = 8
    scene.render.resolution_x = 900
    scene.render.resolution_y = 900
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "8"
    scene.view_settings.look = "AgX - Medium High Contrast"
    scene.world = bpy.data.worlds.new("Dark glass preview world")
    scene.world.color = (0.016, 0.022, 0.032)

    scene.render.film_transparent = False
    scene.render.filepath = str(OUTPUT_PREVIEW)
    bpy.ops.render.render(write_still=True)

    backdrop.hide_render = True
    scene.render.film_transparent = True
    scene.render.filepath = str(OUTPUT_TRANSPARENT)
    bpy.ops.render.render(write_still=True)

    backdrop.hide_render = False
    scene.render.film_transparent = False
    scene.render.filepath = str(OUTPUT_PREVIEW)
    bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT_BLEND))


if __name__ == "__main__":
    main()
