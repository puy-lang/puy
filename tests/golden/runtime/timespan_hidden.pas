program TimeSpanHidden;

// A TTimeSpan the program declares hides System's; System.TTimeSpan still names System's, and
// TInstant's operators take and give System's.

type
  TTimeSpan = record
    Minutes: Integer;
  end;

begin
  var Mine: TTimeSpan := Default(TTimeSpan);
  Mine.Minutes := 5;
  WriteLn(Mine.Minutes);
  var Span := System.TTimeSpan.FromMinutes(Mine.Minutes);
  WriteLn((TInstant.FromUnixSeconds(0) + Span).ToString);
  WriteLn((TInstant.FromUnixSeconds(60) - TInstant.FromUnixSeconds(0)).ToString);
end.
