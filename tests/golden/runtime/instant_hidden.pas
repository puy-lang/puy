program InstantHidden;

// A TInstant the program declares hides System's (the files of System's are then not read);
// System.TInstant still names System's.

type
  TInstant = record
    Ticks: Integer;
  end;

begin
  var Mine: TInstant := Default(TInstant);
  Mine.Ticks := 5;
  WriteLn(Mine.Ticks);
  WriteLn(System.TInstant.FromUnixSeconds(60).ToString);
end.
