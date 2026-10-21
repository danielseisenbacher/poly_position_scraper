package io.github.danielseisenbacher.polywatch.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** Parses the dashboard's sample data and the repo's workflows, so a schema drift fails here. */
class ParsingTest {
    private val root = File(System.getProperty("repoRoot") ?: "../..")
    private val sample = File(root, "site/sample-data")

    @Test
    fun parsesSampleCoreFiles() {
        val meta = json.decodeFromString<Meta>(File(sample, "meta.json").readText())
        assertEquals(10, meta.scoreHistogram.size)
        assertTrue(meta.signalCounts.isNotEmpty())
        assertNotNull(meta.runId)

        val index = json.decodeFromString<List<CandidateRow>>(File(sample, "index.json").readText())
        assertTrue(index.isNotEmpty())
        val withSignals = index.first { it.signals.isNotEmpty() }
        assertTrue(withSignals.signals.all { it.kind.isNotEmpty() && it.strength in 0.0..1.0 })

        val markets = json.decodeFromString<List<MarketView>>(File(sample, "markets.json").readText())
        assertTrue(markets.all { it.conditionId.startsWith("0x") })
        val clusters = json.decodeFromString<List<Cluster>>(File(sample, "clusters.json").readText())
        assertTrue(clusters.all { it.members.size >= 2 && it.links.isNotEmpty() })
        val tape = json.decodeFromString<List<TapeRow>>(File(sample, "activity_recent.json").readText())
        assertTrue(tape.isNotEmpty())
    }

    @Test
    fun parsesEverySampleWallet() {
        val files = File(sample, "wallets").listFiles()!!.filter { it.extension == "json" }
        assertTrue(files.isNotEmpty())
        for (f in files) {
            val w = json.decodeFromString<Wallet>(f.readText())
            assertEquals(f.nameWithoutExtension, w.addr)
            assertNotNull(w.score.total)
        }
    }

    @Test
    fun positionWonMirrorsRust() {
        assertEquals(false, Position(status = "REDEEMABLE_LOST", currentPrice = 1.0).won)
        assertEquals(true, Position(status = "REDEEMABLE", currentPrice = 1.0).won)
        assertEquals(true, Position(status = "CLOSED", currentPrice = 0.9995).won)
        assertEquals(false, Position(status = "CLOSED", currentPrice = 0.0).won)
        assertNull(Position(status = "CLOSED", currentPrice = 0.5).won)
        assertNull(Position(status = "OPEN", currentPrice = 1.0).won)
    }

    @Test
    fun readsDispatchInputsOfRepoWorkflows() {
        val wf = File(root, ".github/workflows")
        val discover = parseDispatchInputs(File(wf, "discover.yml").readText())!!
        assertEquals(listOf("dry_run", "seed_file"), discover.map { it.name })
        assertEquals("boolean", discover[0].type)
        assertEquals("false", discover[0].default)
        assertEquals("", discover[1].default)
        assertTrue(discover[1].description.startsWith("Optional path"))

        val track = parseDispatchInputs(File(wf, "track.yml").readText())!!
        assertEquals(listOf("dry_run"), track.map { it.name })

        assertEquals(emptyList<DispatchInput>(), parseDispatchInputs(File(wf, "maintenance.yml").readText()))
        // build.yml has workflow_dispatch without inputs; pages.yml too.
        assertEquals(emptyList<DispatchInput>(), parseDispatchInputs(File(wf, "pages.yml").readText()))

        val legacy = parseDispatchInputs(File(wf, "legacy-close.yml").readText())!!
        assertEquals("insider-alert", legacy.first { it.name == "old_label" }.default)
        assertEquals("true", legacy.first { it.name == "dry_run" }.default)
    }

    @Test
    fun dispatchParserEdgeCases() {
        assertNull(parseDispatchInputs("on:\n  push:\n    branches: [main]\n"))
        assertEquals(emptyList<DispatchInput>(), parseDispatchInputs("on: [push, workflow_dispatch]\n"))
        val choice = parseDispatchInputs(
            """
            on:
              workflow_dispatch:
                inputs:
                  level:
                    type: choice
                    required: true
                    options:
                      - low
                      - "high"
            jobs: {}
            """.trimIndent(),
        )!!
        assertEquals(listOf("low", "high"), choice.single().options)
        assertTrue(choice.single().required)
    }
}
