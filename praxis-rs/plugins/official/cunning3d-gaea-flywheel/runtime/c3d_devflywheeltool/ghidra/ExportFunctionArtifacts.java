//@category Cunning3D

import java.io.File;
import java.io.FileWriter;
import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.Iterator;
import java.util.List;
import java.util.Set;

import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileOptions;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.util.headless.HeadlessScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.listing.Listing;
import ghidra.program.model.pcode.HighFunction;
import ghidra.program.model.pcode.PcodeOpAST;
import ghidra.program.model.symbol.Symbol;
import ghidra.program.model.symbol.SymbolIterator;

public class ExportFunctionArtifacts extends HeadlessScript {
    private static final class TargetSpec {
        String label;
        String selector;
    }

    private static final class Config {
        File outDir;
        int timeoutSeconds = 120;
        List<TargetSpec> targets = new ArrayList<>();
    }

    @Override
    protected void run() throws Exception {
        if (currentProgram == null) {
            throw new IllegalStateException("No current program.");
        }
        Config config = parseArgs(getScriptArgs());
        if (config.targets.isEmpty()) {
            throw new IllegalArgumentException("At least one target=<label@selector> is required.");
        }
        ensureDirectory(config.outDir);
        DecompInterface decompiler = createDecompiler();
        try {
            if (!decompiler.openProgram(currentProgram)) {
                throw new IllegalStateException("Failed to open decompiler: " + decompiler.getLastMessage());
            }
            for (TargetSpec target : config.targets) {
                exportTarget(config, target, decompiler);
            }
        } finally {
            decompiler.dispose();
        }
    }

    private Config parseArgs(String[] args) {
        Config config = new Config();
        config.outDir = new File(currentProgram.getExecutablePath()).getParentFile();
        for (int index = 0; index < args.length; index++) {
            String argument = args[index];
            if (argument.startsWith("out=")) {
                config.outDir = new File(argument.substring(4));
            } else if ("out".equals(argument)) {
                config.outDir = new File(args[requireValue(args, index++, "out")]);
            } else if (argument.startsWith("timeout=")) {
                config.timeoutSeconds = Integer.parseInt(argument.substring(8));
            } else if ("timeout".equals(argument)) {
                config.timeoutSeconds = Integer.parseInt(args[requireValue(args, index++, "timeout")]);
            } else if (argument.startsWith("target=")) {
                config.targets.add(parseTarget(argument.substring(7)));
            } else if ("target".equals(argument)) {
                config.targets.add(parseTarget(args[requireValue(args, index++, "target")]));
            } else {
                throw new IllegalArgumentException("Unsupported argument: " + argument);
            }
        }
        return config;
    }

    private int requireValue(String[] args, int index, String key) {
        if (index + 1 >= args.length) {
            throw new IllegalArgumentException("Missing value after " + key);
        }
        return index + 1;
    }

    private TargetSpec parseTarget(String raw) {
        TargetSpec target = new TargetSpec();
        int split = raw.indexOf('@');
        target.label = split < 0 ? raw : raw.substring(0, split);
        target.selector = split < 0 ? raw : raw.substring(split + 1);
        if (target.label.isBlank()) {
            target.label = target.selector;
        }
        return target;
    }

    private DecompInterface createDecompiler() {
        DecompInterface decompiler = new DecompInterface();
        decompiler.setOptions(new DecompileOptions());
        decompiler.toggleCCode(true);
        decompiler.toggleSyntaxTree(true);
        decompiler.setSimplificationStyle("decompile");
        return decompiler;
    }

    private void exportTarget(Config config, TargetSpec target, DecompInterface decompiler) throws Exception {
        Function function = resolveFunction(target.selector);
        File targetDir = new File(config.outDir, sanitize(target.label));
        ensureDirectory(targetDir);
        DecompileResults results = decompiler.decompileFunction(function, config.timeoutSeconds, monitor);
        writeText(new File(targetDir, "meta.txt"), metadata(target, function, results));
        writeText(new File(targetDir, "decompiled.c"), decompiled(function, results));
        writeText(new File(targetDir, "pcode.txt"), pcode(function, results));
        writeText(new File(targetDir, "asm.txt"), assembly(function));
        println("Exported " + target.label + " -> " + targetDir.getAbsolutePath());
    }

    private Function resolveFunction(String selector) throws Exception {
        if (selector.startsWith("name:")) {
            return resolveFunctionByName(selector.substring(5));
        }
        Address address = resolveAddress(selector);
        FunctionManager manager = currentProgram.getFunctionManager();
        Function function = manager.getFunctionAt(address);
        if (function == null) {
            function = manager.getFunctionContaining(address);
        }
        if (function == null || function.getBody().getNumAddresses() <= 1) {
            if (function != null) {
                manager.removeFunction(function.getEntryPoint());
            }
            new DisassembleCommand(address, null, true).applyTo(currentProgram, monitor);
            new CreateFunctionCmd(address).applyTo(currentProgram, monitor);
            function = manager.getFunctionAt(address);
        }
        if (function == null) {
            throw new IllegalArgumentException("No function at " + selector);
        }
        return function;
    }

    private Function resolveFunctionByName(String name) {
        List<Function> matches = new ArrayList<>();
        SymbolIterator symbols = currentProgram.getSymbolTable().getSymbolIterator(name, true);
        while (symbols.hasNext()) {
            Symbol symbol = symbols.next();
            Function function = currentProgram.getFunctionManager().getFunctionAt(symbol.getAddress());
            if (function != null) {
                matches.add(function);
            }
        }
        if (matches.size() != 1) {
            throw new IllegalArgumentException("Expected one function named " + name + ", found " + matches.size());
        }
        return matches.get(0);
    }

    private Address resolveAddress(String selector) {
        String value = selector.trim();
        boolean rva = value.startsWith("rva:");
        if (rva) {
            value = value.substring(4);
        } else if (value.startsWith("va:")) {
            value = value.substring(3);
        }
        long parsed = value.startsWith("0x") || value.startsWith("0X")
            ? Long.parseUnsignedLong(value.substring(2), 16)
            : Long.parseLong(value);
        long imageBase = currentProgram.getImageBase().getOffset();
        return toAddr(rva || parsed < imageBase ? imageBase + parsed : parsed);
    }

    private String metadata(TargetSpec target, Function function, DecompileResults results) throws Exception {
        StringBuilder text = new StringBuilder();
        long imageBase = currentProgram.getImageBase().getOffset();
        long entry = function.getEntryPoint().getOffset();
        text.append("Program: ").append(currentProgram.getName()).append('\n');
        text.append("TargetLabel: ").append(target.label).append('\n');
        text.append("Selector: ").append(target.selector).append('\n');
        text.append("FunctionName: ").append(function.getName()).append('\n');
        text.append("Signature: ").append(function.getPrototypeString(true, true)).append('\n');
        text.append("EntryVA: 0x").append(Long.toHexString(entry)).append('\n');
        text.append("EntryRVA: 0x").append(Long.toHexString(entry - imageBase)).append('\n');
        text.append("BodyMin: ").append(function.getBody().getMinAddress()).append('\n');
        text.append("BodyMax: ").append(function.getBody().getMaxAddress()).append('\n');
        text.append("DecompilerCompleted: ").append(results.decompileCompleted()).append('\n');
        text.append("DecompilerMessage: ").append(results.getErrorMessage()).append("\n\n");
        appendFunctions(text, "Callers", function.getCallingFunctions(monitor));
        appendFunctions(text, "Callees", function.getCalledFunctions(monitor));
        return text.toString();
    }

    private void appendFunctions(StringBuilder text, String title, Set<Function> functions) {
        List<Function> ordered = new ArrayList<>(functions);
        Collections.sort(ordered, Comparator.comparingLong(function -> function.getEntryPoint().getOffset()));
        text.append(title).append(":\n");
        for (Function function : ordered) {
            text.append("  ").append(function.getEntryPoint()).append("  ").append(function.getName()).append('\n');
        }
        if (ordered.isEmpty()) {
            text.append("  <none>\n");
        }
        text.append('\n');
    }

    private String decompiled(Function function, DecompileResults results) {
        if (!results.decompileCompleted() || results.getDecompiledFunction() == null) {
            return "// " + function.getName() + "\n// Decompiler failed: " + results.getErrorMessage() + "\n";
        }
        return "// " + function.getName() + "\n" + results.getDecompiledFunction().getC();
    }

    private String pcode(Function function, DecompileResults results) {
        StringBuilder text = new StringBuilder("# " + function.getName() + "\n");
        HighFunction high = results.getHighFunction();
        if (!results.decompileCompleted() || high == null) {
            return text.append("# HighFunction unavailable: ").append(results.getErrorMessage()).append('\n').toString();
        }
        Iterator<PcodeOpAST> iterator = high.getPcodeOps();
        while (iterator.hasNext()) {
            PcodeOpAST op = iterator.next();
            text.append(op.getSeqnum().getTarget()).append(':').append(op.getSeqnum().getTime()).append("  ").append(op).append('\n');
        }
        return text.toString();
    }

    private String assembly(Function function) throws Exception {
        StringBuilder text = new StringBuilder();
        Listing listing = currentProgram.getListing();
        InstructionIterator iterator = listing.getInstructions(function.getBody(), true);
        while (iterator.hasNext()) {
            Instruction instruction = iterator.next();
            text.append(instruction.getAddress()).append("  ").append(hex(instruction.getBytes())).append("  ").append(instruction).append('\n');
        }
        return text.toString();
    }

    private String hex(byte[] bytes) {
        StringBuilder text = new StringBuilder();
        for (int index = 0; index < bytes.length; index++) {
            if (index > 0) {
                text.append(' ');
            }
            text.append(String.format("%02x", bytes[index] & 0xff));
        }
        return text.toString();
    }

    private void ensureDirectory(File directory) throws Exception {
        if ((!directory.exists() && !directory.mkdirs()) || !directory.isDirectory()) {
            throw new IllegalStateException("Cannot create directory " + directory.getAbsolutePath());
        }
    }

    private void writeText(File file, String text) throws Exception {
        try (PrintWriter writer = new PrintWriter(new FileWriter(file, false))) {
            writer.print(text);
        }
    }

    private String sanitize(String value) {
        return value.replaceAll("[^A-Za-z0-9._-]", "_");
    }
}
