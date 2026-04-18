Proposal: The Magical Git Experience with Pre Processed Files

This file is not a feature proposal per se, but the user facing conceptual document that captures what we see as the ideal design for future dodot, that allows us to withhold our goals and principles and impose minimal costs on users to make it all work. 
See the @../proposals/template-expansion.lex and @docs/../proposals/preprocessing-pipeline.lex for context.


1. Source Control and dodot: git always wins

    Dotfiles manager are *primarily* about collecting such files from many locations and centralizing them for proper source control. That is the primary point, from which everything flow, like reproducibility, recovery, history tracking. These are what source control gets, not the dotfiles manager.
    Hence, it's a sensible way to define such programs as a software that facilitates getting your configuration under source control.

2. The Overbearing Solutions

    However, the more advanced ones , in a combination of a complicated design and the proposition of handling things, often overstep and handle some parts of source control. Sometimes it's just a specific use case, sometimes they want you to execute git calls through them or through a especial sub shell. The point being that in some scenario the answer to how do I do task X (a source control one) changes from your vanilla git answer.
    This is a core principle of dodot: "though shall not fuck around with source control" . That is, git does it's job very well, and for whatever shortcomings one may see, there is an ecosystem of tools to augment it. Dotfiles management is something you want for the long haul, probably be with you for decade. Coupling your data, workflows and tooling on a particular solution is bound to cause your more work. Sure, git is such a coupling, but it's likely to think that git's shell live will be much longer small dotfiles manager, and the new comer is likely to be git compatible also. That's why in dodot nothing in source control is done outside git, ever. git diff tells you what has changed, as does history, checkout and everything else. 
    However, what what about the things that are not really in git, or not entirely? 

3. When Source Files and Deploy File Differ

    Some situations, the thing you keep under source control is not the thing that gets deployed? In these cases, while you may still keep the source snugly under git , git diff still tells you what has changed to the source file, but not the deployed one (nor could it?).
    There are a common use cases: injecting secret at run time (to avoid keeping sensitive data in source control), template expanded files and cases like our Plist solution , where you transform the plain text version of the format into the binary one for deployment, keeping both source control and the application requirements happy, gpg encrypted files and so on. 
    In this case, the source file is fine in git, it's just that if you made a version to the deployed file, how does that propagates back to the git source? 
    Many solutions handle this by forcing a workflow change on how you edit your configs , usually with further restrictions on how (which tool), and or having an "apply" step (a change in your workflow really).
    These were strict no-gos for dodot: no apply step, no workflow change, no forcing of config editing tools.
But that works as long as you can keep source -> deployed 1o1, as these can be links , which has these nice properties we want. But for transformed files that breaks. You can't have your cake and eat it too: if these don't match, something has to control changes, hence the workflow, apply step, tooling requirements.

4. dodot: have your cake and eat it.

    We wanted to support all these Transformation use cases, but not give up on principles: no change to workflow or tooling, git is the source of truth.
    Other dotfiles manager are correctly framing the situation: you can't have all these things at the same time. What dodot does, is not about breaking logical facts, but making a few trade-offs that we hope feels like the magic keep able and eatable cake. 
    The answer is a bit unorthodox, as this does requires a bit of reality bending effort, but it works and is true _in spirit_,  to both our principles and the user needs.  Our solution takes some clever bits and a willingness to add to git some tooling.
    The answer is git it self. By using clever smudge and clean filters, and some clever diffing techniques, we can make this work magically.  

    4.1 Representational Transformations

        For perfectly revertible transforms, smudge and filter are all that is needed. These can deterministically describe it's files and hence have a correct git diff and statuses.

    4.2  Generative Transformations
    
        For non revertible transformations, such as template expansions -- which includes secret injection -- there is no deterministically correct way to do so, as more than one changes can result in the same processed output. However, when we think of what dodot does, what user needs, it's possible to count on simple heuristics that allow users to have the benefit without effort, and when ambiguous situations arise, we can have the user confirm the correct choice.
        That stems from the fact that, if you can determine whether the diff in generated form is related to expansion at generation time, then you can ignore these and still see a meaningful diff with the actual interesting bits that should go back to source control.  
        What it means is that for non inject lines, we can very reliably tell changes that do not touch injected ones, and hence we can correctly and assertively state the diff.
        For lines that that do, whoever, we do not. In these cases , what we will do, is to pre fill the file with that information, the original line, the changed line, and let the user decide what is the right call. It's a sort of a merge conflict, except there is no merging, as the changes are not commited yet. 
        We believe that is a good compromise, that can do the expected thing almost always correctly , that is provide the magical solution, given a few compromises.

The Magical BurgerToCow

    The formal truth holds: the problem is unsolvable in a deterministic and general way. However this doesn't need to cover all possible forms and be generalizable. We can leverage the information we have (the source text, the values for the expansion in the template), and knowledge of the templating engine (minijinja) to produce diffs to be done on the original template for many cases, and being explicit about the other ones , and asking the user to resolve it.
    This is done by our burgertocow[github.com/arthur-debert/burgertocow] crate, and it uses interesting shadow map ideas to achieve it. I
    It's assertiveness can greatly be enhanced with a simple best practice of assigning local template variables to the expansion on top of your file, in shell files for example: 
        GH_TOKEN="{{secret("...")}}"
    another way is to use minijinja and set them as template vars: 
        {% set GH_TOKEN secret("...) %}
    :: text:: 
    This technique, which is good practice regardless to make the template easier to Underhand and change isolates expansions into dedicated lines , after which very few cases can't be reliably diffed.

The User Experience

    1. The Git Data Bit

        There are now only two problems left, how to teach git to map / transform these values, and how they fit the regular fit usage. By using git's smudge and clean filters, we can alter and map the values that get used by git (be them diff, status, commits), so you have to install these . 
        This is fairly straight forward and the $dodot helper git-install-filters (does it all) and git-show-filters (outputs the config for you to inspect , setup) will get it done.

    2. The Update Trigger Bit

        The last final hurdle , getting git to use said filters. You see, git does not goes through a file's actual content on each check, instead only doing so if it's modification time stamp is newer than the ref content. This is a smart things, and it makes git much Snappier. But it our case, it poses a problem. 
    When you pre-process your templates, the generated output gets deployed, that is , becomes the configuration file your system will use. If you then update that (in whatever tool or workflow, in our core goals), only the generated one gets updated, and the source's time stamp won't have changed. Then if you git diff it, it will see the old time stamp and it never runs the filters, so the changes never pickup. 
        So we need a way to change the source's timestamps when the deployed content changes. The logic for that is trivial, and can be done fast , we just copy the timestamps from the deployed file into the source and we only do these for the files that need it. Note that the file can have no changed and that is fine, git will do the correct thing, it will just spend the time to read it instead of bypassing it through the time stamp. 
        The tricky part is when to do that. There are various solutions but they all make different trade-offs: 
            1. A custom dodot refresh command: 

                This is the most straight forward one, and it can be used. The trouble with this is that is requires users to change (abet a little) their git workflow, and worse, to remember doing it.

            2. A Pre commit Hook

                While status and diff have no hooks, commit does. Hence we can call the refresh logic there (and give users a one command or Y to have that automated, so the user effort is minimal and done once per machine). 
                However, it only happens when you commit a change, and if you git diff or git status, it won't show up. This is a reasonable solution , that trades away immediate status and diff output by postponing it until the next repo's commit.

            3. A Nice and Simple Hack

                You can set and alias / function (and do it in this repo only with an .env var), that will call dodot refresh first, than git diff / status . 
                These are utterly simple: 
                    git alias ...
                :: shell ::
                And likewise we can do it for your, or provide you the text to add.

            4. A Deamon Of Sorts

                The "perfect" solution would be a file level hook / deamon that checks it , hen runs dodot refresh as needed. We do not think this should be done by dodot, it's too out of scope, too invasive and brings more complexity than we like. 
                But there are still simple solutions , if that is your preferred approach: the well established tool (insert them) you can use the dodot refresh --list-paths command to generate these as needed and have your tool doing it automated. If this solution rocks your boat, say you already leverage thee file watcher, this is also simple , done once only and will produce the magical behavior we want.

            5. Dodot Overstepping Tools And Workflow

                Were dodot to require e explicit apply step for updating changes, or a specific tool or shell to be used for changing values, this could be handled there. But this anathema to our goals and ethos, so this is a hard no. If you are Ok with it, solution 1 works already, just call refresh (but the burden is on you, something we do not feel good about)

        In all, if you are okay with adding the filters and the two local aliases, you're good .
        - On the first deploy (up) of files that use pre processors we will check if these are installed.
        - If not we will ask you if you want us to install it, output the scripts for you to do so, or forgetaboutit.

        We do think that our design does what we set out for: no changes to users or flows, at the cost of installing two alias and filters once , which is guided and can be done for your. The user effort is minimal, the solution is not too intrusive or magic, and it works every time. If you're Ok with it, all that it will require you is a yes once per machine you setup (filters and aliases wont be git controlled, hence not available on new checkouts.)



