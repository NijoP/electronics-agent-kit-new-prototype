// The shared laptop is slow under load: give async test utilities room so flows fail on behaviour, not latency.
import { configure } from "@testing-library/react";

configure({ asyncUtilTimeout: 5000 });
