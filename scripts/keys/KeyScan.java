// Lists the key mappings a Minecraft client jar and Fabric mod jars create, with their default keys,
// for crates/pl-mc/testdata/key-bindings (the official pack's inventory that data/key-bindings.json is
// checked against). Re-run it when the pack's mods change.
//
// Runs a small constant-tracking data flow analysis over every method that constructs a
// net/minecraft/client/KeyMapping (or a subclass), follows wrapper methods whose parameters carry
// the name or the key, and reads nested jars (META-INF/jars). Needs ASM 9.8 or newer (asm, asm-tree,
// asm-analysis; e.g. from the Gradle cache) and Java 21+:
//
//   java -cp "asm.jar;asm-tree.jar;asm-analysis.jar" scripts/keys/KeyScan.java \
//     <versions/26.1.2/26.1.2.jar> <mods/*.jar> > keys.tsv
//
// (":" instead of ";" outside Windows). Output, one line per key mapping (TSV): jar, class.method,
// name, input type, key code, options.txt key, category, note, constructor. Rows noted "name not
// constant" or "key not constant" are built at run time (e.g. Replay Mod's "key.replaymod.<id>"):
// read those with javap. The client jar goes first; the first jar's rows are marked "vanilla:".
import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.zip.*;
import org.objectweb.asm.*;
import org.objectweb.asm.tree.*;
import org.objectweb.asm.tree.analysis.*;

public class KeyScan {
  static final String KM = "net/minecraft/client/KeyMapping";
  static final Map<String, String> SUPERS = new HashMap<>();
  /** jar label -> classes (code kept). */
  static final Map<String, List<ClassNode>> JARS = new LinkedHashMap<>();
  static final List<String> OUT = new ArrayList<>();

  public static void main(String[] args) throws Exception {
    for (int i = 0; i < args.length; i++) {
      String label = (i == 0 ? "vanilla:" : "") + Paths.get(args[i]).getFileName();
      try (InputStream in = Files.newInputStream(Paths.get(args[i]))) {
        readJar(label, in);
      }
    }
    // Wrappers: methods whose parameters reach a KeyMapping constructor's name or key.
    Map<String, int[]> wrappers = new LinkedHashMap<>(); // owner.name+desc -> [nameArg, keyArg]
    for (int round = 0; round < 6; round++) {
      Map<String, int[]> found = new LinkedHashMap<>();
      forEachMethod((jar, cn, mn) -> scan(jar, cn, mn, wrappers, found, false));
      found.keySet().removeAll(wrappers.keySet());
      if (found.isEmpty()) break;
      wrappers.putAll(found);
    }
    forEachMethod((jar, cn, mn) -> scan(jar, cn, mn, wrappers, new LinkedHashMap<>(), true));
    for (String w : wrappers.keySet()) System.err.println("wrapper " + w + " " + Arrays.toString(wrappers.get(w)));
    new LinkedHashSet<>(OUT).forEach(System.out::println);
  }

  interface MethodVisitor3 {
    void visit(String jar, ClassNode cn, MethodNode mn);
  }

  static void forEachMethod(MethodVisitor3 visitor) {
    for (var jar : JARS.entrySet()) {
      for (ClassNode cn : jar.getValue()) {
        for (MethodNode mn : cn.methods) visitor.visit(jar.getKey(), cn, mn);
      }
    }
  }

  static void readJar(String label, InputStream in) throws IOException {
    ZipInputStream zip = new ZipInputStream(in);
    ZipEntry e;
    List<ClassNode> list = new ArrayList<>();
    while ((e = zip.getNextEntry()) != null) {
      String n = e.getName();
      if (n.endsWith(".class") && !n.startsWith("META-INF/versions/")) {
        byte[] b = zip.readAllBytes();
        try {
          ClassNode cn = new ClassNode();
          new ClassReader(b).accept(cn, ClassReader.SKIP_FRAMES);
          if (cn.superName != null) SUPERS.put(cn.name, cn.superName);
          // The client jar is big: only its classes that mention key mappings are analysed.
          if (!label.startsWith("vanilla:") || mentionsKeys(b)) list.add(cn);
        } catch (Exception ex) {
          System.err.println("skip " + label + " " + n + ": " + ex);
        }
      } else if (n.startsWith("META-INF/jars/") && n.endsWith(".jar")) {
        byte[] b = zip.readAllBytes();
        readJar(label + "!" + n.substring(n.lastIndexOf('/') + 1), new ByteArrayInputStream(b));
      }
    }
    JARS.put(label, list);
  }

  static boolean mentionsKeys(byte[] b) {
    String s = new String(b, java.nio.charset.StandardCharsets.ISO_8859_1);
    return s.contains("KeyMapping");
  }

  static boolean isKeyMapping(String cls) {
    for (int i = 0; cls != null && i < 30; i++) {
      if (cls.equals(KM)) return true;
      cls = SUPERS.get(cls);
    }
    return false;
  }

  static void scan(String jar, ClassNode cn, MethodNode mn, Map<String, int[]> wrappers,
      Map<String, int[]> found, boolean report) {
    if (mn.instructions.size() == 0) return;
    boolean relevant = false;
    for (AbstractInsnNode insn : mn.instructions) {
      if (insn instanceof MethodInsnNode min && (isTarget(min) || wrappers.containsKey(key(min)))) {
        relevant = true;
        break;
      }
    }
    if (!relevant) return;
    Frame<CV>[] frames;
    try {
      frames = new Analyzer<>(new Interp()).analyze(cn.name, mn);
    } catch (AnalyzerException ex) {
      if (report) OUT.add(jar + "\t" + cn.name + "." + mn.name + "\t?\t\t\t\t\tanalysis failed: " + ex.getMessage() + "\t");
      return;
    }
    boolean isStatic = (mn.access & Opcodes.ACC_STATIC) != 0;
    for (int i = 0; i < mn.instructions.size(); i++) {
      AbstractInsnNode insn = mn.instructions.get(i);
      if (!(insn instanceof MethodInsnNode min)) continue;
      Frame<CV> f = frames[i];
      if (f == null) continue;
      Type[] at = Type.getArgumentTypes(min.desc);
      CV[] av = new CV[at.length];
      for (int k = 0; k < at.length; k++) av[k] = f.getStack(f.getStackSize() - at.length + k);
      int nameArg = -1, keyArg = -1, typeArg = -1, catArg = -1;
      if (isTarget(min)) {
        for (int k = 0; k < at.length; k++) {
          String d = at[k].getDescriptor();
          if (nameArg < 0 && d.equals("Ljava/lang/String;")) nameArg = k;
          else if (keyArg < 0 && d.equals("I")) keyArg = k;
          else if (typeArg < 0 && d.endsWith("InputConstants$Type;")) typeArg = k;
          else if (catArg < 0 && (d.endsWith("KeyMapping$Category;") || d.equals("Ljava/lang/String;"))) catArg = k;
        }
      } else if (wrappers.containsKey(key(min))) {
        int[] w = wrappers.get(key(min));
        nameArg = w[0];
        keyArg = w[1];
      } else {
        continue;
      }
      CV name = nameArg >= 0 ? av[nameArg] : null;
      CV code = keyArg >= 0 ? av[keyArg] : null;
      // A constructor of a KeyMapping subclass calling super(...) with its own parameters, or any
      // method passing its parameters on: a wrapper; its call sites carry the values.
      boolean nameFromParam = name != null && name.param >= 0;
      boolean codeFromParam = code != null && code.param >= 0;
      if (nameFromParam || codeFromParam) {
        int na = nameFromParam ? argIndex(mn, isStatic, name.param) : -1;
        int ka = codeFromParam ? argIndex(mn, isStatic, code.param) : -1;
        boolean subclassCtor = mn.name.equals("<init>") && isKeyMapping(cn.name);
        if (!subclassCtor) found.put(cn.name + "." + mn.name + mn.desc, new int[] {na, ka});
        if (!report || subclassCtor) continue;
      }
      if (!report) continue;
      String type = typeArg >= 0 ? str(av[typeArg]) : "KEYSYM(default)";
      String cat = catArg >= 0 && catArg != nameArg ? str(av[catArg]) : "";
      String codeStr = code == null ? "" : str(code);
      String keyName = code != null && code.cst instanceof Integer c ? glfw(type, c) : "?";
      String note = (name == null || !(name.cst instanceof String)) ? "name not constant" : "";
      if (code == null || !(code.cst instanceof Integer)) note += (note.isEmpty() ? "" : "; ") + "key not constant";
      OUT.add(jar + "\t" + cn.name.substring(cn.name.lastIndexOf('/') + 1) + "." + mn.name + "\t" + str(name)
          + "\t" + type + "\t" + codeStr + "\t" + keyName + "\t" + cat + "\t" + note + "\t"
          + min.owner.substring(min.owner.lastIndexOf('/') + 1) + "." + min.name);
    }
  }

  static boolean isTarget(MethodInsnNode min) {
    return min.getOpcode() == Opcodes.INVOKESPECIAL && min.name.equals("<init>") && isKeyMapping(min.owner);
  }

  static String key(MethodInsnNode min) {
    return min.owner + "." + min.name + min.desc;
  }

  /** Local slot -> argument index (not counting the receiver). */
  static int argIndex(MethodNode mn, boolean isStatic, int slot) {
    int s = isStatic ? 0 : 1;
    Type[] at = Type.getArgumentTypes(mn.desc);
    for (int k = 0; k < at.length; k++) {
      if (s == slot) return k;
      s += at[k].getSize();
    }
    return -1;
  }

  static String str(CV v) {
    if (v == null) return "";
    if (v.cst == null) return v.param >= 0 ? "<param " + v.param + ">" : "?";
    return v.cst.toString();
  }

  /** The options.txt name of a GLFW key or mouse button (InputConstants). */
  static String glfw(String type, int c) {
    if (type.contains("MOUSE")) {
      return switch (c) {
        case 0 -> "key.mouse.left";
        case 1 -> "key.mouse.right";
        case 2 -> "key.mouse.middle";
        default -> "key.mouse." + (c + 1);
      };
    }
    if (c == -1) return "key.keyboard.unknown";
    if (c >= 65 && c <= 90) return "key.keyboard." + (char) ('a' + c - 65);
    if (c >= 48 && c <= 57) return "key.keyboard." + (char) ('0' + c - 48);
    if (c >= 290 && c <= 314) return "key.keyboard.f" + (c - 289);
    if (c >= 320 && c <= 329) return "key.keyboard.keypad." + (c - 320);
    return switch (c) {
      case 32 -> "key.keyboard.space";
      case 39 -> "key.keyboard.apostrophe";
      case 44 -> "key.keyboard.comma";
      case 45 -> "key.keyboard.minus";
      case 46 -> "key.keyboard.period";
      case 47 -> "key.keyboard.slash";
      case 59 -> "key.keyboard.semicolon";
      case 61 -> "key.keyboard.equal";
      case 91 -> "key.keyboard.left.bracket";
      case 92 -> "key.keyboard.backslash";
      case 93 -> "key.keyboard.right.bracket";
      case 96 -> "key.keyboard.grave.accent";
      case 256 -> "key.keyboard.escape";
      case 257 -> "key.keyboard.enter";
      case 258 -> "key.keyboard.tab";
      case 259 -> "key.keyboard.backspace";
      case 260 -> "key.keyboard.insert";
      case 261 -> "key.keyboard.delete";
      case 262 -> "key.keyboard.right";
      case 263 -> "key.keyboard.left";
      case 264 -> "key.keyboard.down";
      case 265 -> "key.keyboard.up";
      case 266 -> "key.keyboard.page.up";
      case 267 -> "key.keyboard.page.down";
      case 268 -> "key.keyboard.home";
      case 269 -> "key.keyboard.end";
      case 280 -> "key.keyboard.caps.lock";
      case 281 -> "key.keyboard.scroll.lock";
      case 282 -> "key.keyboard.num.lock";
      case 283 -> "key.keyboard.print.screen";
      case 284 -> "key.keyboard.pause";
      case 330 -> "key.keyboard.keypad.decimal";
      case 331 -> "key.keyboard.keypad.divide";
      case 332 -> "key.keyboard.keypad.multiply";
      case 333 -> "key.keyboard.keypad.subtract";
      case 334 -> "key.keyboard.keypad.add";
      case 335 -> "key.keyboard.keypad.enter";
      case 336 -> "key.keyboard.keypad.equal";
      case 340 -> "key.keyboard.left.shift";
      case 341 -> "key.keyboard.left.control";
      case 342 -> "key.keyboard.left.alt";
      case 343 -> "key.keyboard.left.win";
      case 344 -> "key.keyboard.right.shift";
      case 345 -> "key.keyboard.right.control";
      case 346 -> "key.keyboard.right.alt";
      case 347 -> "key.keyboard.right.win";
      case 348 -> "key.keyboard.menu";
      default -> "code " + c;
    };
  }

  /** A value: its size, a known constant (or a symbolic description) and the parameter slot it came from. */
  static final class CV implements Value {
    final int size;
    final Object cst;
    final int param;

    CV(int size, Object cst, int param) {
      this.size = size;
      this.cst = cst;
      this.param = param;
    }

    public int getSize() {
      return size;
    }

    @Override
    public boolean equals(Object o) {
      return o instanceof CV v && v.size == size && v.param == param && Objects.equals(v.cst, cst);
    }

    @Override
    public int hashCode() {
      return Objects.hash(size, cst, param);
    }
  }

  static final class Interp extends Interpreter<CV> {
    Interp() {
      super(Opcodes.ASM9);
    }

    static CV of(Type t) {
      return new CV(t == null ? 1 : t.getSize(), null, -1);
    }

    @Override
    public CV newValue(Type type) {
      if (type == Type.VOID_TYPE) return null;
      return of(type);
    }

    @Override
    public CV newParameterValue(boolean isInstanceMethod, int local, Type type) {
      return new CV(type.getSize(), null, local);
    }

    @Override
    public CV newOperation(AbstractInsnNode insn) {
      int op = insn.getOpcode();
      switch (op) {
        case Opcodes.ICONST_M1, Opcodes.ICONST_0, Opcodes.ICONST_1, Opcodes.ICONST_2, Opcodes.ICONST_3,
            Opcodes.ICONST_4, Opcodes.ICONST_5:
          return new CV(1, op - Opcodes.ICONST_0, -1);
        case Opcodes.LCONST_0, Opcodes.LCONST_1, Opcodes.DCONST_0, Opcodes.DCONST_1:
          return new CV(2, null, -1);
        case Opcodes.BIPUSH, Opcodes.SIPUSH:
          return new CV(1, ((IntInsnNode) insn).operand, -1);
        case Opcodes.LDC: {
          Object c = ((LdcInsnNode) insn).cst;
          int size = (c instanceof Long || c instanceof Double) ? 2 : 1;
          return new CV(size, c, -1);
        }
        case Opcodes.GETSTATIC: {
          FieldInsnNode f = (FieldInsnNode) insn;
          return new CV(Type.getType(f.desc).getSize(), f.owner.substring(f.owner.lastIndexOf('/') + 1) + "." + f.name, -1);
        }
        default:
          return new CV(1, null, -1);
      }
    }

    @Override
    public CV copyOperation(AbstractInsnNode insn, CV value) {
      return value;
    }

    @Override
    public CV unaryOperation(AbstractInsnNode insn, CV value) {
      int op = insn.getOpcode();
      switch (op) {
        case Opcodes.CHECKCAST:
          return value;
        case Opcodes.LNEG, Opcodes.DNEG, Opcodes.I2L, Opcodes.I2D, Opcodes.L2D, Opcodes.F2L, Opcodes.F2D, Opcodes.D2L:
          return new CV(2, null, -1);
        case Opcodes.GETFIELD: {
          FieldInsnNode f = (FieldInsnNode) insn;
          return new CV(Type.getType(f.desc).getSize(), null, -1);
        }
        case Opcodes.IFEQ, Opcodes.IFNE, Opcodes.IFLT, Opcodes.IFGE, Opcodes.IFGT, Opcodes.IFLE,
            Opcodes.IFNULL, Opcodes.IFNONNULL, Opcodes.TABLESWITCH, Opcodes.LOOKUPSWITCH, Opcodes.IRETURN,
            Opcodes.LRETURN, Opcodes.FRETURN, Opcodes.DRETURN, Opcodes.ARETURN, Opcodes.PUTSTATIC,
            Opcodes.ATHROW, Opcodes.MONITORENTER, Opcodes.MONITOREXIT:
          return null;
        default:
          return new CV(1, null, -1);
      }
    }

    @Override
    public CV binaryOperation(AbstractInsnNode insn, CV a, CV b) {
      int op = insn.getOpcode();
      switch (op) {
        case Opcodes.LALOAD, Opcodes.DALOAD, Opcodes.LADD, Opcodes.DADD, Opcodes.LSUB, Opcodes.DSUB, Opcodes.LMUL,
            Opcodes.DMUL, Opcodes.LDIV, Opcodes.DDIV, Opcodes.LREM, Opcodes.DREM, Opcodes.LSHL, Opcodes.LSHR,
            Opcodes.LUSHR, Opcodes.LAND, Opcodes.LOR, Opcodes.LXOR:
          return new CV(2, null, -1);
        case Opcodes.IF_ICMPEQ, Opcodes.IF_ICMPNE, Opcodes.IF_ICMPLT, Opcodes.IF_ICMPGE, Opcodes.IF_ICMPGT,
            Opcodes.IF_ICMPLE, Opcodes.IF_ACMPEQ, Opcodes.IF_ACMPNE, Opcodes.PUTFIELD:
          return null;
        case Opcodes.IADD:
          if (a.cst instanceof Integer x && b.cst instanceof Integer y) return new CV(1, x + y, -1);
          return new CV(1, null, -1);
        default:
          return new CV(1, null, -1);
      }
    }

    @Override
    public CV ternaryOperation(AbstractInsnNode insn, CV a, CV b, CV c) {
      return null;
    }

    @Override
    public CV naryOperation(AbstractInsnNode insn, List<? extends CV> values) {
      if (insn.getOpcode() == Opcodes.MULTIANEWARRAY) return new CV(1, null, -1);
      if (insn instanceof InvokeDynamicInsnNode indy) {
        Type ret = Type.getReturnType(indy.desc);
        if (indy.name.equals("makeConcatWithConstants") && indy.bsmArgs.length > 0 && indy.bsmArgs[0] instanceof String recipe) {
          StringBuilder sb = new StringBuilder();
          int k = 0;
          for (char ch : recipe.toCharArray()) {
            if (ch == '\u0001') {
              CV v = values.get(k++);
              sb.append(v.cst == null ? "<?>" : v.cst.toString());
            } else {
              sb.append(ch);
            }
          }
          return new CV(1, sb.toString(), -1);
        }
        return ret == Type.VOID_TYPE ? null : of(ret);
      }
      MethodInsnNode min = (MethodInsnNode) insn;
      Type ret = Type.getReturnType(min.desc);
      if (ret == Type.VOID_TYPE) return null;
      // InputConstants.UNKNOWN.getValue(): no key (-1).
      if (min.name.equals("getValue") && min.owner.endsWith("InputConstants$Key")
          && "InputConstants.UNKNOWN".equals(values.get(0).cst)) {
        return new CV(1, -1, -1);
      }
      // Symbolic description of calls with constant arguments (categories, identifiers).
      StringBuilder sb = new StringBuilder("CALL:" + min.owner.substring(min.owner.lastIndexOf('/') + 1) + "." + min.name + "(");
      int start = min.getOpcode() == Opcodes.INVOKESTATIC ? 0 : 1;
      boolean known = true;
      for (int k = start; k < values.size(); k++) {
        CV v = values.get(k);
        if (k > start) sb.append(",");
        if (v.cst == null) {
          known = false;
          sb.append("?");
        } else {
          sb.append(v.cst);
        }
      }
      sb.append(")");
      if (start == 1 && values.get(0).cst != null) sb.insert(5, values.get(0).cst + "->");
      return new CV(ret.getSize(), (known || ret.getSort() == Type.OBJECT) ? sb.toString() : null, -1);
    }

    @Override
    public void returnOperation(AbstractInsnNode insn, CV value, CV expected) {}

    @Override
    public CV merge(CV a, CV b) {
      if (a.equals(b)) return a;
      return new CV(Math.min(a.size, b.size), null, -1);
    }
  }
}
