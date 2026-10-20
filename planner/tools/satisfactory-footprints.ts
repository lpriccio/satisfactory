// Floor area per machine in m² (width × length). The game's docs export has no
// building dimensions, so this table is maintained by hand.
// "TSV" = the Size column of the legacy recipes.tsv (preferred when present);
// "wiki" = published building dimensions.

export const FOOTPRINTS: Record<string, number> = {
  SmelterMk1: 54, // TSV (6 × 9)
  ConstructorMk1: 80, // TSV (8 × 10)
  AssemblerMk1: 150, // TSV (10 × 15)
  FoundryMk1: 90, // TSV (10 × 9)
  ManufacturerMk1: 360, // TSV (18 × 20)
  OilRefinery: 200, // TSV (10 × 20)
  Packager: 64, // TSV (8 × 8)
  Blender: 288, // TSV (18 × 16)
  HadronCollider: 912, // TSV (24 × 38), Particle Accelerator
  Converter: 256, // TSV (16 × 16)
  QuantumEncoder: 1200, // TSV (wiki gives 22 × 48 = 1056)
  GeneratorCoal: 260, // TSV (10 × 26)
  GeneratorFuel: 400, // TSV (20 × 20)
  GeneratorNuclear: 1548, // TSV (36 × 43)
  GeneratorBiomass_Automated: 64, // wiki, approximate (8 × 8)
};
