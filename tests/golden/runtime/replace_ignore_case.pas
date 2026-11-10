program ReplaceIgnoreCase;

// rfIgnoreCase of StringReplace matches by the Unicode case mapping table without language rules
// (semantics.md 5.3, requirements §5.11): `Ä` / `ä` and `Σ` / `σ` match (the final `ς` is a
// letter of its own), `İ` (whose lowercase is two characters) and a lone surrogate match only
// themselves, and an empty pattern replaces nothing. SameText and CompareText stay ASCII only.

uses System.SysUtils;

const
  All = [TReplaceFlag.rfReplaceAll, TReplaceFlag.rfIgnoreCase];

begin
  WriteLn(StringReplace('ÄbäB', 'ä', '-', All));
  WriteLn(StringReplace('ΣΑΣ σας', 'σ', 's', All));
  WriteLn(StringReplace('İi', 'i', '-', All));
  WriteLn(StringReplace('aÄa', 'Ä', '-', [TReplaceFlag.rfIgnoreCase]));
  WriteLn(StringReplace('x😀y😀', '😀', '!', All));
  WriteLn(Length(StringReplace('a' + #$D800 + 'b', #$D800, '', All)));
  WriteLn(StringReplace('abc', '', '-', All));
  WriteLn(SameText('Ä', 'ä'), ' ', CompareText('Ä', 'ä') <> 0);
end.
