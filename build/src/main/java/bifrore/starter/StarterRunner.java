package bifrore.starter;

import lombok.extern.slf4j.Slf4j;
import org.apache.commons.cli.*;

import java.io.File;

@Slf4j
public class StarterRunner {
    static {
        Thread.setDefaultUncaughtExceptionHandler(
                (t, e) -> log.error("Caught an exception in thread[{}]", t.getName(), e));
    }

    public static <S extends BaseStarter> void run(Class<S> starterClazz, String[] args) {
        CommandLineParser parser = new DefaultParser();
        try {
            CommandLine cmd = parser.parse(cliOptions(), args);
            File confFile = new File(cmd.getOptionValue("c"));
            if (!confFile.exists()) {
                throw new RuntimeException("Conf file does not exist: " + cmd.getOptionValue("c"));
            }
            BaseStarter starter = starterClazz.getDeclaredConstructor().newInstance();
            starter.init(starter.buildConfig(confFile));
            starter.start();
            Thread shutdownThread = new Thread(starter::stop);
            shutdownThread.setName("shutdown");
            Runtime.getRuntime().addShutdownHook(shutdownThread);
        } catch (Throwable e) {
            log.error("Caught an exception in thread[{}]", Thread.currentThread().getName(), e);
        }
    }

    private static Options cliOptions() {
        return new Options()
                .addOption(Option.builder()
                        .option("c")
                        .longOpt("conf")
                        .desc("the conf file for Starter")
                        .hasArg(true)
                        .optionalArg(false)
                        .argName("CONF_FILE")
                        .required(true)
                        .build());
    }
}
