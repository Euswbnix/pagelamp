// Settings ▸ AI's model on the mock: it reads what the facade says and re-reads after each
// change; an API key goes to the facade and nowhere else (only its last 4 characters come back);
// failures are kept by what failed.

import Foundation
import PageLamp
import PageLampKit
import PageLampModel
import Testing

@Suite("AI settings model")
@MainActor
struct AiSettingsModelTests {
    func loaded(_ scenario: MockScenario) async -> (AiSettingsModel, MockService) {
        let service = MockService(scenario: scenario, timing: .instant, calendar: TestClock.calendar, now: { TestClock.now })
        let ai = AiSettingsModel(service: service, clock: { TestClock.now }, calendar: TestClock.calendar)
        await ai.load()
        return (ai, service)
    }

    @Test("nothing set up: the key presets, this computer's servers, the months and the usage")
    func loadsEmpty() async throws {
        let (ai, _) = await loaded(.demo)
        #expect(ai.statusFailure == nil)
        #expect(ai.providerBackends.isEmpty && ai.usableBackends.isEmpty && !ai.hasApiKey)
        #expect(ai.keyPresets.allSatisfy(\.needsKey))
        #expect(ai.keyPresets.map(\.id).contains("openai") && !ai.keyPresets.map(\.id).contains("ollama"))
        let servers = try #require(ai.localServers)
        #expect(servers.map(\.kind) == [.ollama, .lmStudio])
        #expect(servers.map(\.running) == [true, false])
        #expect(ai.outputLanguage != nil)
        #expect(ai.months.count == 13)
        #expect(ai.months.first == "2026-09-01" && ai.months.last == "2025-09-01")
        #expect(ai.usageMonth == "2026-09-01" && ai.usage != nil)
        #expect(!ai.codexSignedIn)
    }

    @Test("adding a key: the key isn't kept, the backend asks for its disclosure, then is ready")
    func addKey() async throws {
        let (ai, _) = await loaded(.demo)
        let preset = try #require(ai.keyPresets.first { $0.id == "openai" })
        let key = "sk-demo-never-kept-7Qx2"
        let record = try #require(await ai.addProvider(preset: preset, baseUrl: "", key: key))
        #expect(record.keyLast4 == "7Qx2")
        // Nothing the model holds has the key in it.
        for child in Mirror(reflecting: ai).children {
            #expect(!String(describing: child.value).contains(key), "\(child.label ?? "?") holds the key")
        }
        let backend = try #require(ai.providerBackends.first)
        #expect(backend.state == .needsDisclosure && ai.hasApiKey)
        #expect(ai.provider(of: backend)?.keyLast4 == "7Qx2")
        #expect(ai.backend(key: "provider:openai")?.label == backend.label)
        #expect(await ai.acknowledge(backend))
        #expect(ai.providerBackends.first?.state == .ready)
        #expect(ai.models["provider:openai"]?.isEmpty == false)
        // Replacing it keeps only the new last 4.
        #expect(await ai.replaceKey(providerId: "openai", key: "sk-demo-new-Wx9Y"))
        #expect(ai.status?.providers.first?.keyLast4 == "Wx9Y")
    }

    @Test("a refused key: the failure by its code, and nothing added")
    func refusedKey() async throws {
        let (ai, _) = await loaded(.demo)
        let preset = try #require(ai.keyPresets.first { $0.id == "openai" })
        #expect(await ai.addProvider(preset: preset, baseUrl: "", key: "sk-bad-0000") == nil)
        #expect(ai.keyFailure?.modelError == .authRejected)
        #expect(await ai.addProvider(preset: preset, baseUrl: "", key: "sk-sp-demo-0000") == nil)
        #expect(ai.keyFailure?.blocked == .codingPlanKey)
        #expect(await ai.addProvider(preset: preset, baseUrl: "", key: "   ") == nil)
        #expect(ai.keyFailure?.kind == .invalid)
        #expect(ai.providerBackends.isEmpty)
        ai.clearKeyFailure()
        #expect(ai.keyFailure == nil)
    }

    @Test("a running local server is added once; it then shows as added")
    func localServer() async throws {
        let (ai, _) = await loaded(.demo)
        let ollama = try #require(ai.localServers?.first { $0.kind == .ollama })
        #expect(!ai.isAdded(ollama))
        let record = try #require(await ai.useLocalServer(ollama))
        #expect(record.onDevice && record.keyLast4 == nil)
        #expect(ai.isAdded(ollama))
        #expect(ai.providerBackends.first?.kind == .local)
        #expect(!ai.hasApiKey)
    }

    @Test("a feature's model and effort are saved and re-read; Test reports the probe")
    func featureModels() async throws {
        let (ai, _) = await loaded(.aiKey)
        let choice = try #require(ai.choice(for: .studyPlan))
        #expect(ai.chosenModel(for: .studyPlan)?.id == choice.model)
        await ai.setEffort(.studyPlan, .high)
        #expect(ai.choice(for: .studyPlan)?.effort == .high && ai.choice(for: .studyPlan)?.model == choice.model)
        await ai.test(.studyPlan)
        guard case .report(let report)? = ai.tests[.studyPlan] else {
            Issue.record("expected a probe report")
            return
        }
        #expect(report.ok && report.latencyMs > 0)
        // A new choice clears the last test; none unsets it and keeps no failure.
        await ai.setModel(.studyPlan, backend: nil, model: nil)
        #expect(ai.choice(for: .studyPlan) == nil && ai.tests[.studyPlan] == nil)
        #expect(ai.modelChoiceFailures[.studyPlan] == nil)
        // A model the backend doesn't offer is refused, for that feature only.
        await ai.setModel(.weeklyNote, backend: choice.backend, model: "no-such-model")
        #expect(ai.modelChoiceFailures[.weeklyNote] != nil && ai.modelChoiceFailures[.studyPlan] == nil)
    }

    @Test("a server that can't be reached: Test says why")
    func unreachable() async throws {
        let (ai, _) = await loaded(.aiErrors)
        await ai.test(.studyPlan)
        guard case .report(let report)? = ai.tests[.studyPlan] else {
            Issue.record("expected a probe report")
            return
        }
        #expect(!report.ok && report.error == .rateLimited)
    }

    @Test("the budget is saved or removed, and re-read")
    func budget() async throws {
        let (ai, _) = await loaded(.aiKey)
        #expect(await ai.saveBudget(microUsd: 2_500_000))
        #expect(ai.status?.budget.monthlyMicroUsd == 2_500_000)
        #expect(ai.usage?.budget.monthlyMicroUsd == 2_500_000)
        #expect(await ai.saveBudget(microUsd: nil))
        #expect(ai.status?.budget.monthlyMicroUsd == nil)
    }

    @Test("usage per month; the last month asked for wins")
    func usage() async throws {
        let (ai, _) = await loaded(.aiKey)
        let current = try #require(ai.usage)
        #expect(!current.rows.isEmpty)
        await ai.loadUsage(month: "2025-09-01")
        #expect(ai.usageMonth == "2025-09-01" && ai.usage?.rows.isEmpty == true)
    }

    @Test("removing a provider, then everything")
    func removal() async throws {
        let (ai, _) = await loaded(.aiKey)
        let provider = try #require(ai.status?.providers.first)
        #expect(await ai.removeProvider(provider.providerId))
        #expect(ai.status?.providers.contains { $0.providerId == provider.providerId } == false)
        #expect(await ai.removeProvider(provider.providerId) == false)
        #expect(ai.removeFailure?.kind == .notFound)

        let (all, _) = await loaded(.aiKey)
        #expect(await all.removeAll())
        #expect(all.status?.providers.isEmpty == true && all.providerBackends.isEmpty)
        #expect(all.status?.budget.monthlyMicroUsd == 5_000_000)
        #expect(all.tests.isEmpty && all.removeAllFailure == nil)
    }
}
